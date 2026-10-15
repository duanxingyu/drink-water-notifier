export type DrinkUnit = "cup" | "ml" | "sip";

export type UpdateInfo = {
  version: string;
  body: string | null;
};

export type DrinkHistoryDay = {
  date: string;
  amount: number;
  unit: DrinkUnit;
  label: string;
  convertedLabel: string | null;
  isToday: boolean;
};

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
  highIntakeMode: boolean;
  deskGoalMl: number;
  intakeMlToday: number;
  goalRemainingMl: number;
  goalProgress: number;
  glassesToday: number;
  intakeLabel: string;
  unitLabel: string;
  quickDrinkShortcut: boolean;
  daySummaryEnabled: boolean;
  paused: boolean;
  pausedUntil: string | null;
  configPath: string;
  configWarning: string | null;
  appVersion: string;
  updateAvailable: UpdateInfo | null;
};

export type ReminderKind = "drink" | "summary";

export type ReminderPayload = {
  kind: ReminderKind;
  token: number;
  autoDismissSeconds: number;
  snoozeMinutes: number;
  glassesToday: number;
  drinkUnit: DrinkUnit;
  drinkAmount: number;
  intakeLabel: string;
  drinkPrompt: string;
  drinkAck: string;
  highIntakeMode: boolean;
  deskGoalMl: number;
  intakeMlToday: number;
  goalLabel: string | null;
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
  highIntakeMode: boolean;
  deskGoalMl: number;
  quickDrinkShortcut: boolean;
  daySummaryEnabled: boolean;
};

/** 1 杯 ≈ 350 毫升 */
export const ML_PER_CUP = 350;
/** 1 口 ≈ 80 毫升 */
export const ML_PER_SIP = 80;

/** 工位时段推荐目标（毫升），与后端默认值一致 */
export const RECOMMENDED_DESK_GOAL_ML = 1500;
export const RECOMMENDED_HIGH_INTAKE_INTERVAL = 25;
export const RECOMMENDED_HIGH_INTAKE_AMOUNT_ML = 200;
export const DESK_GOAL_ML_MIN = 500;
export const DESK_GOAL_ML_MAX = 4000;

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function defaultAmountFor(unit: DrinkUnit): number {
  return unit === "ml" ? 200 : 1;
}

export function toMl(amount: number, unit: DrinkUnit): number {
  if (unit === "ml") return amount;
  if (unit === "cup") return amount * ML_PER_CUP;
  return amount * ML_PER_SIP;
}

export function fromMl(ml: number, unit: DrinkUnit): number {
  if (unit === "ml") return Math.round(ml);
  if (unit === "cup") return Math.round(ml / ML_PER_CUP);
  return Math.round(ml / ML_PER_SIP);
}

export function convertDrinkAmount(amount: number, from: DrinkUnit, to: DrinkUnit): number {
  if (from === to) return amount;
  const converted = fromMl(toMl(amount, from), to);
  if (to === "ml") return Math.min(1000, Math.max(10, converted));
  return Math.min(20, Math.max(1, converted));
}

export function convertIntakeTotal(amount: number, from: DrinkUnit, to: DrinkUnit): number {
  if (from === to) return amount;
  return Math.max(0, fromMl(toMl(amount, from), to));
}

export function counterpartUnit(unit: DrinkUnit): DrinkUnit {
  return unit === "ml" ? "cup" : "ml";
}

export function formatIntake(amount: number, unit: DrinkUnit): string {
  if (unit === "cup") return `${amount} 杯`;
  if (unit === "ml") return `${amount} 毫升`;
  if (amount === 0) return "0 口";
  if (amount === 1) return "一口";
  if (amount === 2) return "两口";
  return `${amount} 口`;
}

export function formatIntakeApprox(
  amount: number,
  from: DrinkUnit,
  to: DrinkUnit,
): string | null {
  if (from === to) return null;
  const converted = convertIntakeTotal(amount, from, to);
  if (converted === 0 && amount > 0) {
    if (to !== "ml") {
      const ml = toMl(amount, from);
      if (ml > 0) return `约 ${ml} 毫升`;
    }
    return null;
  }
  return `约 ${formatIntake(converted, to)}`;
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
  highIntakeMode: false,
  deskGoalMl: RECOMMENDED_DESK_GOAL_ML,
  intakeMlToday: 1050,
  goalRemainingMl: 450,
  goalProgress: 1050 / RECOMMENDED_DESK_GOAL_ML,
  glassesToday: 3,
  intakeLabel: "3 杯",
  unitLabel: "杯",
  quickDrinkShortcut: true,
  daySummaryEnabled: true,
  paused: false,
  pausedUntil: null,
  configPath: "预览模式 · 尚未写入系统配置目录",
  configWarning: null,
  appVersion: "0.1.0",
  updateAvailable: null,
};

let memory = structuredClone(preview);
let previewHistory: DrinkHistoryDay[] = [
  {
    date: new Date().toISOString().slice(0, 10),
    amount: 3,
    unit: "cup",
    label: "3 杯",
    convertedLabel: "约 1050 毫升",
    isToday: true,
  },
  {
    date: offsetDate(-1),
    amount: 600,
    unit: "ml",
    label: "600 毫升",
    convertedLabel: "约 2 杯",
    isToday: false,
  },
  {
    date: offsetDate(-2),
    amount: 5,
    unit: "sip",
    label: "5 口",
    convertedLabel: "约 400 毫升",
    isToday: false,
  },
];
const listeners = new Set<(snap: Snapshot) => void>();

function offsetDate(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
}

function publish() {
  const next = structuredClone(memory);
  listeners.forEach((listener) => listener(next));
}

function refreshPreviewLabels() {
  memory.intakeLabel = formatIntake(memory.glassesToday, memory.drinkUnit);
  memory.unitLabel = unitLabel(memory.drinkUnit);
}

function refreshPreviewHistoryToday() {
  const today = new Date().toISOString().slice(0, 10);
  const rest = previewHistory.filter((day) => !day.isToday);
  previewHistory = [
    {
      date: today,
      amount: memory.glassesToday,
      unit: memory.drinkUnit,
      label: formatIntake(memory.glassesToday, memory.drinkUnit),
      convertedLabel: formatIntakeApprox(
        memory.glassesToday,
        memory.drinkUnit,
        counterpartUnit(memory.drinkUnit),
      ),
      isToday: true,
    },
    ...rest.map((day) => ({
      ...day,
      convertedLabel:
        day.unit === memory.drinkUnit
          ? formatIntakeApprox(day.amount, day.unit, counterpartUnit(memory.drinkUnit))
          : formatIntakeApprox(day.amount, day.unit, memory.drinkUnit),
    })),
  ];
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
    if (input.drinkUnit !== memory.drinkUnit) {
      memory.glassesToday = convertIntakeTotal(
        memory.glassesToday,
        memory.drinkUnit,
        input.drinkUnit,
      );
    }
    const intakeMlToday = toMl(memory.glassesToday, input.drinkUnit);
    memory = {
      ...memory,
      ...input,
      glassesToday: memory.glassesToday,
      intakeMlToday,
      goalRemainingMl: Math.max(0, input.deskGoalMl - intakeMlToday),
      goalProgress:
        input.deskGoalMl > 0
          ? Math.min(1, intakeMlToday / input.deskGoalMl)
          : 1,
      quickDrinkShortcut: input.quickDrinkShortcut,
      daySummaryEnabled: input.daySummaryEnabled,
      paused: memory.paused,
      pausedUntil: memory.pausedUntil,
      configWarning: null,
    };
    refreshPreviewLabels();
    refreshPreviewHistoryToday();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("save_settings", { input });
}

export async function drink(): Promise<Snapshot> {
  if (!isTauri()) {
    memory.glassesToday += memory.drinkAmount;
    memory.intakeMlToday = toMl(memory.glassesToday, memory.drinkUnit);
    memory.goalRemainingMl = Math.max(0, memory.deskGoalMl - memory.intakeMlToday);
    memory.goalProgress =
      memory.deskGoalMl > 0
        ? Math.min(1, memory.intakeMlToday / memory.deskGoalMl)
        : 1;
    refreshPreviewLabels();
    refreshPreviewHistoryToday();
    publish();
    return structuredClone(memory);
  }
  return invoke<Snapshot>("drink");
}

export async function getDrinkHistory(): Promise<DrinkHistoryDay[]> {
  if (!isTauri()) {
    refreshPreviewHistoryToday();
    return structuredClone(previewHistory);
  }
  return invoke<DrinkHistoryDay[]>("get_drink_history");
}

export async function clearDrinkHistory(): Promise<DrinkHistoryDay[]> {
  if (!isTauri()) {
    previewHistory = previewHistory.filter((day) => day.isToday);
    return structuredClone(previewHistory);
  }
  return invoke<DrinkHistoryDay[]>("clear_drink_history");
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

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (!isTauri()) return null;
  return invoke<UpdateInfo | null>("check_for_update");
}

export async function installUpdate(): Promise<void> {
  if (!isTauri()) return;
  await invoke<void>("install_update");
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

function defaultDrinkPrompt(unit: DrinkUnit): string {
  if (unit === "sip") return "离开屏幕一小会儿，喝一口水。";
  if (unit === "ml") return "离开屏幕一小会儿，喝一点水。";
  return "离开屏幕一小会儿，喝一杯水。";
}

/** 浏览器预览用的简化上下文文案（与 core 规则大致对齐）。 */
export function previewContextualPrompt(
  unit: DrinkUnit,
  intakeMlToday: number,
  deskGoalMl: number,
  highIntakeMode: boolean,
): string {
  if (highIntakeMode && deskGoalMl > 0 && intakeMlToday < deskGoalMl * 0.35) {
    if (unit === "sip") return "进度偏慢，先喝一口。";
    if (unit === "ml") return "进度偏慢，先补一点水。";
    return "进度偏慢，这杯先补上。";
  }
  if (intakeMlToday === 0) {
    if (unit === "sip") return "下午了还没记过，先喝一口。";
    if (unit === "ml") return "下午了还没记过，先喝一点。";
    return "下午了还没记过，先喝一杯。";
  }
  if (deskGoalMl > 0 && intakeMlToday >= deskGoalMl) {
    return "目标到了，再润一下也不错。";
  }
  return defaultDrinkPrompt(unit);
}

export function previewReminder(): ReminderPayload {
  const intakeMlToday = toMl(memory.glassesToday, memory.drinkUnit);
  return {
    kind: "drink",
    token: 1,
    autoDismissSeconds: memory.autoDismissSeconds,
    snoozeMinutes: memory.snoozeMinutes,
    glassesToday: memory.glassesToday,
    drinkUnit: memory.drinkUnit,
    drinkAmount: memory.drinkAmount,
    intakeLabel: memory.intakeLabel,
    drinkPrompt: previewContextualPrompt(
      memory.drinkUnit,
      intakeMlToday,
      memory.deskGoalMl,
      memory.highIntakeMode,
    ),
    drinkAck:
      memory.drinkUnit === "sip"
        ? "记下这一口"
        : memory.drinkUnit === "ml"
          ? "记下这些水"
          : "记下这一杯",
    highIntakeMode: memory.highIntakeMode,
    deskGoalMl: memory.deskGoalMl,
    intakeMlToday,
    goalLabel: memory.highIntakeMode
      ? `工位目标 ${intakeMlToday}/${memory.deskGoalMl} 毫升`
      : null,
  };
}

/** 近 7 天补齐（无记录为 0），用于历史周视图。 */
export function buildWeekBars(
  days: DrinkHistoryDay[],
  todayIso = new Date().toISOString().slice(0, 10),
): Array<{ date: string; ml: number; isToday: boolean; weekday: string }> {
  const byDate = new Map(days.map((day) => [day.date, toMl(day.amount, day.unit)]));
  const labels = ["日", "一", "二", "三", "四", "五", "六"];
  const result: Array<{ date: string; ml: number; isToday: boolean; weekday: string }> = [];
  const anchor = new Date(`${todayIso}T12:00:00`);
  for (let i = 6; i >= 0; i -= 1) {
    const d = new Date(anchor);
    d.setDate(anchor.getDate() - i);
    const iso = d.toISOString().slice(0, 10);
    result.push({
      date: iso,
      ml: byDate.get(iso) ?? 0,
      isToday: iso === todayIso,
      weekday: labels[d.getDay()] ?? "",
    });
  }
  return result;
}

export function summarizeWeek(bars: Array<{ ml: number }>): { daysLogged: number; avgMl: number } {
  const logged = bars.filter((bar) => bar.ml > 0);
  const total = bars.reduce((sum, bar) => sum + bar.ml, 0);
  return {
    daysLogged: logged.length,
    avgMl: logged.length === 0 ? 0 : Math.round(total / 7),
  };
}
