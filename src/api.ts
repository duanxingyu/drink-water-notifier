export type DrinkUnit = "cup" | "ml" | "sip";

export type Snapshot = {
  workdays: string[];
  start: string;
  end: string;
  intervalMinutes: number;
  soundEnabled: boolean;
  volume: number;
  autoDismissSeconds: number;
  snoozeMinutes: number;
  launchAtLogin: boolean;
  respectChineseHolidays: boolean;
  drinkUnit: DrinkUnit;
  drinkAmount: number;
  glassesToday: number;
  intakeLabel: string;
  unitLabel: string;
  paused: boolean;
  pausedUntil: string | null;
  configPath: string;
  configWarning: string | null;
};

export type ReminderPayload = {
  token: number;
  autoDismissSeconds: number;
  snoozeMinutes: number;
  glassesToday: number;
  drinkUnit: DrinkUnit;
  drinkAmount: number;
  intakeLabel: string;
  drinkPrompt: string;
  drinkAck: string;
};

export type SettingsInput = {
  workdays: string[];
  start: string;
  end: string;
  intervalMinutes: number;
  soundEnabled: boolean;
  volume: number;
  autoDismissSeconds: number;
  snoozeMinutes: number;
  launchAtLogin: boolean;
  respectChineseHolidays: boolean;
  drinkUnit: DrinkUnit;
  drinkAmount: number;
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function defaultAmountFor(unit: DrinkUnit): number {
  return unit === "ml" ? 200 : 1;
}

export function formatIntake(amount: number, unit: DrinkUnit): string {
  if (unit === "cup") return `${amount} 杯`;
  if (unit === "ml") return `${amount} 毫升`;
  if (amount === 0) return "0 口";
  if (amount === 1) return "一口";
  if (amount === 2) return "两口";
  return `${amount} 口`;
}

export function unitLabel(unit: DrinkUnit): string {
  if (unit === "cup") return "杯";
  if (unit === "ml") return "毫升";
  return "口";
}

const preview: Snapshot = {
  workdays: ["Mon", "Tue", "Wed", "Thu", "Fri"],
  start: "09:30",
  end: "18:30",
  intervalMinutes: 30,
  soundEnabled: true,
  volume: 0.7,
  autoDismissSeconds: 20,
  snoozeMinutes: 5,
  launchAtLogin: false,
  respectChineseHolidays: true,
  drinkUnit: "cup",
  drinkAmount: 1,
  glassesToday: 3,
  intakeLabel: "3 杯",
  unitLabel: "杯",
  paused: false,
  pausedUntil: null,
  configPath: "预览模式 · 尚未写入系统配置目录",
  configWarning: null,
};

let memory = structuredClone(preview);
const listeners = new Set<(snap: Snapshot) => void>();

function publish() {
  const next = structuredClone(memory);
  listeners.forEach((listener) => listener(next));
}

function refreshPreviewLabels() {
  memory.intakeLabel = formatIntake(memory.glassesToday, memory.drinkUnit);
  memory.unitLabel = unitLabel(memory.drinkUnit);
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: call } = await import("@tauri-apps/api/core");
  return call<T>(command, args);
}

export async function getSnapshot(): Promise<Snapshot> {
  if (!isTauri()) return structuredClone(memory);
  return invoke<Snapshot>("get_snapshot");
}

export async function activeReminder(): Promise<ReminderPayload | null> {
  if (!isTauri()) return null;
  return invoke<ReminderPayload | null>("active_reminder");
}

export async function saveSettings(input: SettingsInput): Promise<Snapshot> {
  if (!isTauri()) {
    memory = {
      ...memory,
      ...input,
      paused: memory.paused,
      pausedUntil: memory.pausedUntil,
      glassesToday: memory.glassesToday,
      configWarning: null,
    };
    refreshPreviewLabels();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("save_settings", { input });
}

export async function drink(): Promise<Snapshot> {
  if (!isTauri()) {
    memory.glassesToday += memory.drinkAmount;
    refreshPreviewLabels();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("drink");
}

export async function snooze(): Promise<Snapshot> {
  if (!isTauri()) return structuredClone(memory);
  return invoke<Snapshot>("snooze");
}

export async function dismiss(): Promise<Snapshot> {
  if (!isTauri()) return structuredClone(memory);
  return invoke<Snapshot>("dismiss");
}

export async function pauseHour(): Promise<Snapshot> {
  if (!isTauri()) {
    const until = new Date(Date.now() + 60 * 60 * 1000);
    memory.paused = true;
    memory.pausedUntil = until.toISOString();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("pause_hour");
}

export async function pauseToday(): Promise<Snapshot> {
  if (!isTauri()) {
    const until = new Date();
    until.setHours(24, 0, 0, 0);
    memory.paused = true;
    memory.pausedUntil = until.toISOString();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("pause_today");
}

export async function resume(): Promise<Snapshot> {
  if (!isTauri()) {
    memory.paused = false;
    memory.pausedUntil = null;
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("resume");
}

export async function remindNow(): Promise<Snapshot> {
  if (!isTauri()) return structuredClone(memory);
  return invoke<Snapshot>("remind_now");
}

export async function onSnapshot(cb: (snap: Snapshot) => void): Promise<() => void> {
  if (!isTauri()) {
    listeners.add(cb);
    return () => listeners.delete(cb);
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<Snapshot>("snapshot", (event) => cb(event.payload));
  return unlisten;
}

export async function onReminder(cb: (payload: ReminderPayload) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ReminderPayload>("reminder", (event) => cb(event.payload));
  return unlisten;
}

export function previewReminder(): ReminderPayload {
  return {
    token: 1,
    autoDismissSeconds: memory.autoDismissSeconds,
    snoozeMinutes: memory.snoozeMinutes,
    glassesToday: memory.glassesToday,
    drinkUnit: memory.drinkUnit,
    drinkAmount: memory.drinkAmount,
    intakeLabel: memory.intakeLabel,
    drinkPrompt:
      memory.drinkUnit === "sip"
        ? "离开屏幕一小会儿，喝一口水。"
        : memory.drinkUnit === "ml"
          ? "离开屏幕一小会儿，喝一点水。"
          : "离开屏幕一小会儿，喝一杯水。",
    drinkAck:
      memory.drinkUnit === "sip"
        ? "记下这一口"
        : memory.drinkUnit === "ml"
          ? "记下这些水"
          : "记下这一杯",
  };
}
