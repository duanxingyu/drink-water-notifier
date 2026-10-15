import { useEffect, useRef, useState } from "react";

import "@/fonts.css";
import "@/settings.css";

import {
  checkForUpdate,
  clearDrinkHistory,
  convertDrinkAmount,
  convertIntakeTotal,
  counterpartUnit,
  defaultAmountFor,
  DESK_GOAL_ML_MAX,
  DESK_GOAL_ML_MIN,
  formatIntake,
  formatIntakeApprox,
  buildWeekBars,
  getDrinkHistory,
  getSnapshot,
  installUpdate,
  isTauri,
  onSnapshot,
  pauseHour,
  pauseToday,
  RECOMMENDED_DESK_GOAL_ML,
  RECOMMENDED_HIGH_INTAKE_AMOUNT_ML,
  RECOMMENDED_HIGH_INTAKE_INTERVAL,
  remindNow,
  resume,
  saveSettings,
  summarizeWeek,
  type DrinkHistoryDay,
  type DrinkUnit,
  type SettingsInput,
  type Snapshot,
  type UpdateInfo,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";

const DAYS: Array<[string, string]> = [
  ["Mon", "一"],
  ["Tue", "二"],
  ["Wed", "三"],
  ["Thu", "四"],
  ["Fri", "五"],
  ["Sat", "六"],
  ["Sun", "日"],
];

const UNITS: Array<[DrinkUnit, string]> = [
  ["cup", "杯"],
  ["ml", "毫升"],
  ["sip", "口"],
];

function toForm(snap: Snapshot): SettingsInput {
  return {
    workdays: snap.workdays,
    start: snap.start,
    end: snap.end,
    intervalMinutes: snap.intervalMinutes,
    soundEnabled: snap.soundEnabled,
    volume: snap.volume,
    autoDismissSeconds: snap.autoDismissSeconds,
    snoozeMinutes: snap.snoozeMinutes,
    launchAtLogin: snap.launchAtLogin,
    respectChineseHolidays: snap.respectChineseHolidays,
    drinkUnit: snap.drinkUnit,
    drinkAmount: snap.drinkAmount,
    highIntakeMode: snap.highIntakeMode,
    deskGoalMl: snap.deskGoalMl,
    quickDrinkShortcut: snap.quickDrinkShortcut,
    daySummaryEnabled: snap.daySummaryEnabled,
  };
}

function pauseText(iso: string | null, paused: boolean): string | null {
  if (!paused || !iso) return null;
  const when = new Date(iso);
  if (Number.isNaN(when.getTime())) return "提醒已暂停";
  const midnight = new Date();
  midnight.setHours(24, 0, 0, 0);
  if (Math.abs(when.getTime() - midnight.getTime()) < 2000) return "今天不再提醒";
  const clock = when.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  return `已暂停，${clock} 继续`;
}

function clampAmount(unit: DrinkUnit, amount: number): number {
  if (!Number.isFinite(amount)) return defaultAmountFor(unit);
  const rounded = Math.round(amount);
  if (unit === "ml") return Math.min(1000, Math.max(10, rounded));
  return Math.min(20, Math.max(1, rounded));
}

function clampInt(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}

function numberFieldValue(value: number): number | "" {
  return Number.isFinite(value) ? value : "";
}

function parseNumberInput(raw: string): number {
  return raw === "" ? Number.NaN : Number(raw);
}

function formatHistoryDate(iso: string, isToday: boolean): string {
  if (isToday) return "今天";
  const day = new Date(`${iso}T12:00:00`);
  if (Number.isNaN(day.getTime())) return iso;
  const yesterday = new Date();
  yesterday.setHours(12, 0, 0, 0);
  yesterday.setDate(yesterday.getDate() - 1);
  if (
    day.getFullYear() === yesterday.getFullYear() &&
    day.getMonth() === yesterday.getMonth() &&
    day.getDate() === yesterday.getDate()
  ) {
    return "昨天";
  }
  return `${day.getMonth() + 1}月${day.getDate()}日`;
}

export function Settings() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [form, setForm] = useState<SettingsInput | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateToast, setUpdateToast] = useState<{
    text: string;
    tone: "ok" | "error" | "progress";
  } | null>(null);
  const [updateDialog, setUpdateDialog] = useState<UpdateInfo | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyDays, setHistoryDays] = useState<DrinkHistoryDay[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyConfirmClear, setHistoryConfirmClear] = useState(false);
  const [historyClearing, setHistoryClearing] = useState(false);
  /** 各单位上次填过的「每次喝多少」，切换单位时不要丢回默认值。 */
  const amountByUnit = useRef<Partial<Record<DrinkUnit, number>>>({});
  /** 用户点过「稍后」的版本，避免同版本反复弹窗。 */
  const dismissedUpdate = useRef<string | null>(null);
  const updateToastTimer = useRef<number | null>(null);

  function showUpdateToast(text: string, tone: "ok" | "error" | "progress", autoHideMs = 0) {
    if (updateToastTimer.current != null) {
      window.clearTimeout(updateToastTimer.current);
      updateToastTimer.current = null;
    }
    setUpdateToast({ text, tone });
    if (autoHideMs > 0) {
      updateToastTimer.current = window.setTimeout(() => {
        setUpdateToast(null);
        updateToastTimer.current = null;
      }, autoHideMs);
    }
  }

  function clearUpdateToast() {
    if (updateToastTimer.current != null) {
      window.clearTimeout(updateToastTimer.current);
      updateToastTimer.current = null;
    }
    setUpdateToast(null);
  }

  async function load() {
    setLoading(true);
    try {
      const next = await getSnapshot();
      const nextForm = toForm(next);
      amountByUnit.current[nextForm.drinkUnit] = nextForm.drinkAmount;
      setSnap(next);
      setForm(nextForm);
      setSaved(false);
      setError(next.configWarning);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let unlistenSnapshot = () => {};
    let unlistenOpened = () => {};
    let stop = false;
    void (async () => {
      unlistenSnapshot = await onSnapshot((next) => {
        if (stop) return;
        setSnap(next);
        setError(next.configWarning);
      });
      if (isTauri()) {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenOpened = await listen("settings-opened", () => {
          if (stop) return;
          // 重新打开设置时若有待更新，再弹一次（用户可点「稍后」关掉）。
          dismissedUpdate.current = null;
          void load();
        });
      }
      if (!stop) await load();
    })();
    return () => {
      stop = true;
      unlistenSnapshot();
      unlistenOpened();
      if (updateToastTimer.current != null) {
        window.clearTimeout(updateToastTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    const info = snap?.updateAvailable;
    if (!info) return;
    if (dismissedUpdate.current === info.version) return;
    setUpdateDialog(info);
  }, [snap?.updateAvailable]);

  function patch(partial: Partial<SettingsInput>) {
    setSaved(false);
    setForm((current) => {
      if (!current) return current;
      const next = { ...current, ...partial };
      if (
        (partial.drinkAmount !== undefined || partial.drinkUnit !== undefined) &&
        Number.isFinite(next.drinkAmount)
      ) {
        amountByUnit.current[next.drinkUnit] = next.drinkAmount;
      }
      return next;
    });
  }

  function toggleDay(code: string) {
    if (!form) return;
    const has = form.workdays.includes(code);
    const workdays = has ? form.workdays.filter((day) => day !== code) : [...form.workdays, code];
    patch({ workdays });
  }

  function setUnit(unit: DrinkUnit) {
    if (!form || form.drinkUnit === unit) return;
    if (Number.isFinite(form.drinkAmount)) {
      amountByUnit.current[form.drinkUnit] = form.drinkAmount;
    }
    const remembered = amountByUnit.current[unit];
    const amount = Number.isFinite(remembered)
      ? remembered!
      : convertDrinkAmount(
          Number.isFinite(form.drinkAmount) ? form.drinkAmount : 1,
          form.drinkUnit,
          unit,
        );
    patch({ drinkUnit: unit, drinkAmount: amount });
  }

  /** 开启高摄入模式时套用内置推荐：目标 1500 ml、每次 200 ml、间隔 25 分钟。 */
  function applyRecommendedHighIntake() {
    amountByUnit.current.ml = RECOMMENDED_HIGH_INTAKE_AMOUNT_ML;
    patch({
      highIntakeMode: true,
      deskGoalMl: RECOMMENDED_DESK_GOAL_ML,
      drinkUnit: "ml",
      drinkAmount: RECOMMENDED_HIGH_INTAKE_AMOUNT_ML,
      intervalMinutes: RECOMMENDED_HIGH_INTAKE_INTERVAL,
    });
  }

  function setHighIntakeMode(enabled: boolean) {
    if (!form) return;
    if (enabled) {
      applyRecommendedHighIntake();
      return;
    }
    patch({ highIntakeMode: false });
  }

  async function openHistory() {
    setHistoryOpen(true);
    setHistoryConfirmClear(false);
    setHistoryLoading(true);
    try {
      const days = await getDrinkHistory();
      setHistoryDays(days);
    } catch (err) {
      showUpdateToast(err instanceof Error ? err.message : String(err), "error", 4800);
      setHistoryOpen(false);
    } finally {
      setHistoryLoading(false);
    }
  }

  async function onClearHistory() {
    if (!historyConfirmClear) {
      setHistoryConfirmClear(true);
      return;
    }
    setHistoryClearing(true);
    try {
      const days = await clearDrinkHistory();
      setHistoryDays(days);
      setHistoryConfirmClear(false);
      showUpdateToast("已清除历史记录", "ok", 3200);
    } catch (err) {
      showUpdateToast(err instanceof Error ? err.message : String(err), "error", 4800);
    } finally {
      setHistoryClearing(false);
    }
  }

  function amountHint(unit: DrinkUnit): string {
    if (unit === "ml") return "每次点「喝了」加上的毫升数，10 到 1000。";
    if (unit === "sip") return "每次点「喝了」加上几口，1 到 20。一口、两口会按口语显示。";
    return "每次点「喝了」加上几杯，1 到 20。";
  }

  async function onSave() {
    if (!form || saving) return;
    setSaving(true);
    setSaved(false);
    const payload: SettingsInput = {
      ...form,
      drinkAmount: clampAmount(form.drinkUnit, form.drinkAmount),
      intervalMinutes: clampInt(form.intervalMinutes, 1, 240, 30),
      autoDismissSeconds: clampInt(form.autoDismissSeconds, 5, 180, 20),
      snoozeMinutes: clampInt(form.snoozeMinutes, 1, 60, 5),
      deskGoalMl: clampInt(
        form.deskGoalMl,
        DESK_GOAL_ML_MIN,
        DESK_GOAL_ML_MAX,
        RECOMMENDED_DESK_GOAL_ML,
      ),
    };
    try {
      const next = await saveSettings(payload);
      const nextForm = toForm(next);
      amountByUnit.current[nextForm.drinkUnit] = nextForm.drinkAmount;
      setSnap(next);
      setForm(nextForm);
      // 配置已写入即算保存成功；开机启动等旁路警告不否定保存结果。
      setError(next.configWarning);
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  function openUpdateDialog(info: UpdateInfo) {
    dismissedUpdate.current = null;
    setUpdateDialog(info);
  }

  function dismissUpdateDialog() {
    if (updateDialog) {
      dismissedUpdate.current = updateDialog.version;
    }
    setUpdateDialog(null);
  }

  async function onCheckUpdate() {
    setCheckingUpdate(true);
    showUpdateToast("正在检查更新…", "progress");
    try {
      const info = await checkForUpdate();
      const next = await getSnapshot();
      setSnap(next);
      if (info) {
        clearUpdateToast();
        openUpdateDialog(info);
      } else {
        showUpdateToast("已是最新版本", "ok", 3200);
      }
    } catch (err) {
      showUpdateToast(err instanceof Error ? err.message : String(err), "error", 4800);
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function onInstallUpdate() {
    setInstallingUpdate(true);
    showUpdateToast("正在下载并安装更新…", "progress");
    try {
      await installUpdate();
    } catch (err) {
      showUpdateToast(err instanceof Error ? err.message : String(err), "error", 4800);
      setInstallingUpdate(false);
    }
  }

  if (loading || !form || !snap) {
    return (
      <main className="settings">
        <div className="settings-atmosphere" aria-hidden="true" />
        <div className="settings-inner">
          <div className="glass-panel glass-panel-compact">
            <p>{error ? error : "正在读取设置…"}</p>
            {error ? (
              <Button variant="outline" onClick={() => void load()}>
                重试
              </Button>
            ) : null}
          </div>
        </div>
      </main>
    );
  }

  const banner = pauseText(snap.pausedUntil, snap.paused);
  const todayAmount = convertIntakeTotal(snap.glassesToday, snap.drinkUnit, form.drinkUnit);
  const todayLabel = formatIntake(todayAmount, form.drinkUnit);
  const todayApprox = formatIntakeApprox(
    todayAmount,
    form.drinkUnit,
    counterpartUnit(form.drinkUnit),
  );
  const intakeMlToday = convertIntakeTotal(snap.glassesToday, snap.drinkUnit, "ml");
  const deskGoalMl = Number.isFinite(form.deskGoalMl)
    ? form.deskGoalMl
    : RECOMMENDED_DESK_GOAL_ML;
  const goalProgress = deskGoalMl > 0 ? Math.min(1, intakeMlToday / deskGoalMl) : 1;
  const goalRemaining = Math.max(0, deskGoalMl - intakeMlToday);
  const weekBars = buildWeekBars(historyDays);
  const weekStats = summarizeWeek(weekBars);
  const weekMaxMl = Math.max(...weekBars.map((bar) => bar.ml), 1);

  return (
    <main className="settings">
      <div className="settings-atmosphere" aria-hidden="true" />
      <div className="settings-inner">
        <header className="glass-panel glass-hero">
          <p className="kicker">RUNDI</p>
          <h1>润滴</h1>
          <p className="lede">
            工作日里，到点就轻轻提醒你喝水。关掉这个窗口后，它还在托盘里。提醒默认在右下角，可拖到别处。
          </p>
        </header>

        <section className="glass-panel count-block" aria-label="今日饮水量">
          <div className="count-head">
            <div>
              <div className="count-num">
                {form.drinkUnit === "ml"
                  ? String(todayAmount)
                  : String(todayAmount).padStart(2, "0")}
              </div>
              <div className="count-caption">
                今日已喝 · {todayLabel}
                {todayApprox ? `（${todayApprox}）` : ""}
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              className="glass-chip"
              onClick={() => void openHistory()}
            >
              历史记录
            </Button>
          </div>
          {form.highIntakeMode ? (
            <div className="goal-track" aria-label="工位饮水目标进度">
              <div className="goal-track-head">
                <span>
                  工位目标 {intakeMlToday}/{deskGoalMl} 毫升
                </span>
                <span>
                  {goalRemaining === 0 ? "已达标" : `还差 ${goalRemaining} 毫升`}
                </span>
              </div>
              <div className="goal-bar" role="progressbar" aria-valuenow={Math.round(goalProgress * 100)} aria-valuemin={0} aria-valuemax={100}>
                <div className="goal-bar-fill" style={{ width: `${Math.round(goalProgress * 100)}%` }} />
              </div>
              <p className="hint">
                只统计电脑前喝的水，不覆盖起床、通勤与睡前；全天量请遵医嘱另行补足。
              </p>
            </div>
          ) : null}
          <p className="hint">
            {todayAmount === 0
              ? "今天还没有记录。点「喝了」才会计数。"
              : `点「喝了」会加 ${form.drinkAmount} ${form.drinkUnit === "ml" ? "毫升" : form.drinkUnit === "sip" ? "口" : "杯"}。`}
          </p>
          <p className="hint">换算：1 杯 = 350 毫升，1 口 ≈ 80 毫升。</p>
        </section>

        <section className="glass-panel glass-group" aria-label="工位高摄入">
          <div className="row">
            <div>
              <Label htmlFor="high-intake">工位高摄入模式</Label>
              <p className="hint">
                适合需要多喝水的场景（如结石防护）。默认工位目标{" "}
                {RECOMMENDED_DESK_GOAL_ML} 毫升，落后时会略加密提醒。
              </p>
            </div>
            <Switch
              id="high-intake"
              checked={form.highIntakeMode}
              onCheckedChange={(checked) => setHighIntakeMode(checked)}
            />
          </div>
          {form.highIntakeMode ? (
            <>
              <div className="field">
                <Label htmlFor="desk-goal">工位目标（毫升）</Label>
                <Input
                  id="desk-goal"
                  type="number"
                  min={DESK_GOAL_ML_MIN}
                  max={DESK_GOAL_ML_MAX}
                  step={50}
                  className="glass-input"
                  value={numberFieldValue(form.deskGoalMl)}
                  onChange={(event) =>
                    patch({ deskGoalMl: parseNumberInput(event.target.value) })
                  }
                  onBlur={() =>
                    patch({
                      deskGoalMl: clampInt(
                        form.deskGoalMl,
                        DESK_GOAL_ML_MIN,
                        DESK_GOAL_ML_MAX,
                        RECOMMENDED_DESK_GOAL_ML,
                      ),
                    })
                  }
                />
                <p className="hint">
                  推荐默认 {RECOMMENDED_DESK_GOAL_ML} 毫升（约全日目标的一半，仅桌前时段）。
                </p>
              </div>
              <div className="field">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="glass-chip"
                  onClick={() => applyRecommendedHighIntake()}
                >
                  恢复推荐设置
                </Button>
                <p className="hint">
                  目标 {RECOMMENDED_DESK_GOAL_ML} ml · 每次{" "}
                  {RECOMMENDED_HIGH_INTAKE_AMOUNT_ML} ml · 间隔{" "}
                  {RECOMMENDED_HIGH_INTAKE_INTERVAL} 分钟。
                </p>
              </div>
            </>
          ) : null}
        </section>

        {banner ? (
          <div className="banner glass-panel glass-panel-compact">
            <span>{banner}</span>
            <Button
              variant="outline"
              size="sm"
              className="glass-chip"
              onClick={() => void resume().then(setSnap)}
            >
              继续提醒
            </Button>
          </div>
        ) : null}

        <section className="glass-panel glass-group">
          <div className="field">
            <Label>计量单位</Label>
            <div className="units" role="group" aria-label="计量单位">
              {UNITS.map(([code, label]) => (
                <button
                  key={code}
                  type="button"
                  className={form.drinkUnit === code ? "unit on" : "unit"}
                  aria-pressed={form.drinkUnit === code}
                  onClick={() => setUnit(code)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="field">
            <Label htmlFor="amount">
              每次喝多少（
              {form.drinkUnit === "ml" ? "毫升" : form.drinkUnit === "sip" ? "口" : "杯"}）
            </Label>
            <Input
              id="amount"
              type="number"
              min={form.drinkUnit === "ml" ? 10 : 1}
              max={form.drinkUnit === "ml" ? 1000 : 20}
              step={1}
              className="glass-input"
              value={numberFieldValue(form.drinkAmount)}
              onChange={(event) => patch({ drinkAmount: parseNumberInput(event.target.value) })}
              onBlur={() => patch({ drinkAmount: clampAmount(form.drinkUnit, form.drinkAmount) })}
            />
            <p className="hint">{amountHint(form.drinkUnit)}</p>
          </div>

          <div className="field">
            <Label>工作日</Label>
            <div className="days" role="group" aria-label="工作日">
              {DAYS.map(([code, label]) => (
                <button
                  key={code}
                  type="button"
                  className={form.workdays.includes(code) ? "day on" : "day"}
                  aria-pressed={form.workdays.includes(code)}
                  onClick={() => toggleDay(code)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="switch-row">
            <div>
              <Label htmlFor="cn-holiday">遵循中国节假日</Label>
              <p className="hint">法定放假不提醒；调休补班日会提醒，即使那天是周末。</p>
            </div>
            <Switch
              id="cn-holiday"
              checked={form.respectChineseHolidays}
              onCheckedChange={(checked) => patch({ respectChineseHolidays: checked })}
            />
          </div>
        </section>

        <section className="glass-panel glass-group">
          <div className="row">
            <div className="field">
              <Label htmlFor="start">开始</Label>
              <Input
                id="start"
                type="text"
                inputMode="numeric"
                placeholder="09:30"
                spellCheck={false}
                className="glass-input"
                value={form.start}
                onChange={(event) => patch({ start: event.target.value })}
              />
            </div>
            <div className="field">
              <Label htmlFor="end">结束</Label>
              <Input
                id="end"
                type="text"
                inputMode="numeric"
                placeholder="18:30"
                spellCheck={false}
                className="glass-input"
                value={form.end}
                onChange={(event) => patch({ end: event.target.value })}
              />
            </div>
          </div>
          <p className="hint">只在开始到结束之间提醒。结束那一刻不再弹出。</p>

          <div className="field">
            <Label htmlFor="interval">间隔（分钟）</Label>
            <Input
              id="interval"
              type="number"
              min={1}
              max={240}
              step={1}
              className="glass-input"
              value={numberFieldValue(form.intervalMinutes)}
              onChange={(event) => patch({ intervalMinutes: parseNumberInput(event.target.value) })}
              onBlur={() => patch({ intervalMinutes: clampInt(form.intervalMinutes, 1, 240, 30) })}
            />
            <p className="hint">1 到 240。进入时段时先提醒一次，之后按这个间隔。</p>
          </div>

          <div className="row">
            <div className="field">
              <Label htmlFor="dismiss">自动关闭（秒）</Label>
              <Input
                id="dismiss"
                type="number"
                min={5}
                max={180}
                step={1}
                className="glass-input"
                value={numberFieldValue(form.autoDismissSeconds)}
                onChange={(event) =>
                  patch({ autoDismissSeconds: parseNumberInput(event.target.value) })
                }
                onBlur={() =>
                  patch({ autoDismissSeconds: clampInt(form.autoDismissSeconds, 5, 180, 20) })
                }
              />
            </div>
            <div className="field">
              <Label htmlFor="snooze">稍后提醒（分钟）</Label>
              <Input
                id="snooze"
                type="number"
                min={1}
                max={60}
                step={1}
                className="glass-input"
                value={numberFieldValue(form.snoozeMinutes)}
                onChange={(event) => patch({ snoozeMinutes: parseNumberInput(event.target.value) })}
                onBlur={() => patch({ snoozeMinutes: clampInt(form.snoozeMinutes, 1, 60, 5) })}
              />
            </div>
          </div>
        </section>

        <section className="glass-panel glass-group">
          <div className="field">
            <div className="switch-row">
              <Label htmlFor="sound">提示音</Label>
              <Switch
                id="sound"
                checked={form.soundEnabled}
                onCheckedChange={(checked) => patch({ soundEnabled: checked })}
              />
            </div>
            <Label htmlFor="volume">音量 {Math.round(form.volume * 100)}</Label>
            <Slider
              id="volume"
              min={0}
              max={100}
              step={1}
              value={[Math.round(form.volume * 100)]}
              onValueChange={(value) => patch({ volume: (value[0] ?? 0) / 100 })}
              disabled={!form.soundEnabled}
              aria-label="音量"
            />
            <p className="hint">提醒出现时播放一声自己生成的水滴声。</p>
          </div>

          <div className="switch-row">
            <div>
              <Label htmlFor="login">开机时启动</Label>
              <p className="hint">
                登录后在后台运行。须在已安装的 Rundi 里打开并点「保存设置」；开发版不会改系统开机项，以免把安装版注册表冲掉。
              </p>
            </div>
            <Switch
              id="login"
              checked={form.launchAtLogin}
              onCheckedChange={(checked) => patch({ launchAtLogin: checked })}
            />
          </div>

          <div className="switch-row">
            <div>
              <Label htmlFor="quick-drink">快捷键记水</Label>
              <p className="hint">Ctrl/Cmd+Shift+D 立刻记一次当前杯量。托盘也有「刚才喝了」。</p>
            </div>
            <Switch
              id="quick-drink"
              checked={form.quickDrinkShortcut}
              onCheckedChange={(checked) => patch({ quickDrinkShortcut: checked })}
            />
          </div>

          <div className="switch-row">
            <div>
              <Label htmlFor="day-summary">下班小结</Label>
              <p className="hint">工作日到达结束时间后，弹出一次今日饮水小结（无提示音）。</p>
            </div>
            <Switch
              id="day-summary"
              checked={form.daySummaryEnabled}
              onCheckedChange={(checked) => patch({ daySummaryEnabled: checked })}
            />
          </div>
        </section>

        <section className="glass-panel glass-actions">
          {error ? <p className="error">{error}</p> : null}
          <p className="hint save-status">{saved ? "已保存。" : "\u00a0"}</p>

          <div className="actions">
            <Button
              onClick={() => void onSave()}
              disabled={saving}
              className="disabled:opacity-100 glass-primary"
            >
              保存设置
            </Button>
            <Button variant="outline" className="glass-chip" onClick={() => void remindNow()}>
              立即提醒
            </Button>
            <Button
              variant="outline"
              className="glass-chip"
              disabled={checkingUpdate || installingUpdate}
              onClick={() => void onCheckUpdate()}
            >
              {checkingUpdate ? "正在检查…" : "检查更新"}
            </Button>
            <div className="row">
              <Button
                variant="ghost"
                className="glass-ghost"
                onClick={() => void pauseHour().then(setSnap)}
              >
                暂停 1 小时
              </Button>
              <Button
                variant="ghost"
                className="glass-ghost"
                onClick={() => void pauseToday().then(setSnap)}
              >
                今日暂停
              </Button>
            </div>
          </div>

          {snap.updateAvailable && !updateDialog ? (
            <p className="hint update-inline">
              有新版本 {snap.updateAvailable.version} ·{" "}
              <button
                type="button"
                className="linkish"
                disabled={installingUpdate}
                onClick={() => openUpdateDialog(snap.updateAvailable!)}
              >
                查看更新
              </button>
            </p>
          ) : null}
          <p className="path">版本 {snap.appVersion}</p>
          <p className="path">配置文件 {snap.configPath}</p>
        </section>
      </div>

      {updateToast ? (
        <div
          className={`update-toast update-toast-${updateToast.tone}`}
          role="status"
          aria-live="polite"
        >
          <span>{updateToast.text}</span>
          {updateToast.tone !== "progress" ? (
            <button
              type="button"
              className="update-toast-close"
              aria-label="关闭提示"
              onClick={clearUpdateToast}
            >
              关闭
            </button>
          ) : null}
        </div>
      ) : null}

      {updateDialog ? (
        <div
          className="update-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="update-dialog-title"
        >
          <button
            type="button"
            className="update-modal-backdrop"
            aria-label="关闭更新提示"
            disabled={installingUpdate}
            onClick={dismissUpdateDialog}
          />
          <div className="update-modal-card glass-panel">
            <p className="update-modal-kicker">发现新版本</p>
            <h2 id="update-dialog-title">润滴 {updateDialog.version}</h2>
            <p className="update-modal-lede">
              当前版本 {snap.appVersion}。下载安装后会自动重启应用。
            </p>
            {updateDialog.body ? (
              <div className="update-modal-notes">{updateDialog.body}</div>
            ) : null}
            {installingUpdate ? (
              <p className="hint update-modal-progress">正在下载并安装更新…</p>
            ) : null}
            <div className="update-modal-actions">
              <Button
                disabled={installingUpdate}
                onClick={() => void onInstallUpdate()}
                className="disabled:opacity-100"
              >
                {installingUpdate ? "安装中…" : "立即更新"}
              </Button>
              <Button
                variant="outline"
                disabled={installingUpdate}
                onClick={dismissUpdateDialog}
              >
                稍后
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {historyOpen ? (
        <div
          className="update-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="history-dialog-title"
        >
          <button
            type="button"
            className="update-modal-backdrop"
            aria-label="关闭历史记录"
            disabled={historyClearing}
            onClick={() => {
              setHistoryOpen(false);
              setHistoryConfirmClear(false);
            }}
          />
          <div className="update-modal-card history-modal-card glass-panel">
            <p className="update-modal-kicker">喝水记录</p>
            <h2 id="history-dialog-title">历史记录</h2>
            <p className="update-modal-lede">按天查看过去的饮水量。跨天后会自动归档。</p>
            {historyLoading ? (
              <p className="hint">正在读取…</p>
            ) : (
              <>
                <div className="week-chart" aria-label="近七天饮水量">
                  <div className="week-chart-bars">
                    {weekBars.map((bar) => (
                      <div
                        key={bar.date}
                        className={bar.isToday ? "week-bar-col today" : "week-bar-col"}
                      >
                        <div className="week-bar-track">
                          <div
                            className="week-bar-fill"
                            style={{ height: `${Math.round((bar.ml / weekMaxMl) * 100)}%` }}
                          />
                        </div>
                        <span className="week-bar-label">{bar.weekday}</span>
                      </div>
                    ))}
                  </div>
                  <p className="hint week-chart-summary">
                    近 7 天已记 {weekStats.daysLogged} 天 · 日均约 {weekStats.avgMl} 毫升
                  </p>
                </div>
                {historyDays.length === 0 ? (
                  <p className="hint">还没有记录。从今天起点「喝了」会自动保存。</p>
                ) : (
                  <ul className="history-list">
                    {historyDays.map((day) => (
                      <li key={day.date} className="history-item">
                        <div>
                          <div className="history-date">
                            {formatHistoryDate(day.date, day.isToday)}
                            <span className="history-date-iso">{day.date}</span>
                          </div>
                          <div className="history-label">{day.label}</div>
                          {day.convertedLabel ? (
                            <div className="history-converted">{day.convertedLabel}</div>
                          ) : null}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </>
            )}
            {historyConfirmClear ? (
              <p className="hint history-confirm">
                将清除过去的记录，今日计数会保留，且不可恢复。再点一次确认。
              </p>
            ) : null}
            <div className="update-modal-actions">
              <Button
                variant="outline"
                disabled={historyLoading || historyClearing || historyDays.every((d) => d.isToday)}
                onClick={() => void onClearHistory()}
              >
                {historyClearing
                  ? "清除中…"
                  : historyConfirmClear
                    ? "确认清除"
                    : "清除历史"}
              </Button>
              <Button
                disabled={historyClearing}
                onClick={() => {
                  setHistoryOpen(false);
                  setHistoryConfirmClear(false);
                }}
              >
                关闭
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </main>
  );
}
