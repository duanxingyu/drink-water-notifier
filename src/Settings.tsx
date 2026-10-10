import { useEffect, useRef, useState } from "react";

import {
  checkForUpdate,
  defaultAmountFor,
  getSnapshot,
  installUpdate,
  isTauri,
  onSnapshot,
  pauseHour,
  pauseToday,
  remindNow,
  resume,
  saveSettings,
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
    const amount = Number.isFinite(remembered) ? remembered! : defaultAmountFor(unit);
    patch({ drinkUnit: unit, drinkAmount: amount });
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
        <div className="settings-inner">
          <p>{error ? error : "正在读取设置…"}</p>
          {error ? (
            <Button variant="outline" onClick={() => void load()}>
              重试
            </Button>
          ) : null}
        </div>
      </main>
    );
  }

  const banner = pauseText(snap.pausedUntil, snap.paused);

  return (
    <main className="settings">
      <div className="settings-inner">
        <header>
          <p className="kicker">RUNDI</p>
          <h1>润滴</h1>
          <p className="lede">工作日里，到点就轻轻提醒你喝水。关掉这个窗口后，它还在托盘里。提醒默认在右下角，可拖到别处。</p>
        </header>

        <section className="count-block" aria-label="今日饮水量">
          <div>
            <div className="count-num">
              {form.drinkUnit === "ml"
                ? String(snap.glassesToday)
                : String(snap.glassesToday).padStart(2, "0")}
            </div>
            <div className="count-caption">今日已喝 · {snap.intakeLabel}</div>
          </div>
          <p className="hint">
            {snap.glassesToday === 0
              ? "今天还没有记录。点「喝了」才会计数。"
              : `点「喝了」会加 ${form.drinkAmount} ${form.drinkUnit === "ml" ? "毫升" : form.drinkUnit === "sip" ? "口" : "杯"}。`}
          </p>
        </section>

        {banner ? (
          <div className="banner">
            <span>{banner}</span>
            <Button variant="outline" size="sm" onClick={() => void resume().then(setSnap)}>
              继续提醒
            </Button>
          </div>
        ) : null}

        <section className="field">
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
        </section>

        <div className="field">
          <Label htmlFor="amount">每次喝多少（{form.drinkUnit === "ml" ? "毫升" : form.drinkUnit === "sip" ? "口" : "杯"}）</Label>
          <Input
            id="amount"
            type="number"
            min={form.drinkUnit === "ml" ? 10 : 1}
            max={form.drinkUnit === "ml" ? 1000 : 20}
            step={1}
            value={numberFieldValue(form.drinkAmount)}
            onChange={(event) => patch({ drinkAmount: parseNumberInput(event.target.value) })}
            onBlur={() => patch({ drinkAmount: clampAmount(form.drinkUnit, form.drinkAmount) })}
          />
          <p className="hint">{amountHint(form.drinkUnit)}</p>
        </div>

        <section className="field">
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
        </section>

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

        <div className="row">
          <div className="field">
            <Label htmlFor="start">开始</Label>
            <Input
              id="start"
              type="text"
              inputMode="numeric"
              placeholder="09:30"
              spellCheck={false}
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
              value={numberFieldValue(form.snoozeMinutes)}
              onChange={(event) => patch({ snoozeMinutes: parseNumberInput(event.target.value) })}
              onBlur={() => patch({ snoozeMinutes: clampInt(form.snoozeMinutes, 1, 60, 5) })}
            />
          </div>
        </div>

        <section className="field">
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
        </section>

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

        {error ? <p className="error">{error}</p> : null}
        <p className="hint save-status">{saved ? "已保存。" : "\u00a0"}</p>

        <div className="actions">
          <Button
            onClick={() => void onSave()}
            disabled={saving}
            className="disabled:opacity-100"
          >
            保存设置
          </Button>
          <Button variant="outline" onClick={() => void remindNow()}>
            立即提醒
          </Button>
          <Button
            variant="outline"
            disabled={checkingUpdate || installingUpdate}
            onClick={() => void onCheckUpdate()}
          >
            {checkingUpdate ? "正在检查…" : "检查更新"}
          </Button>
          <div className="row">
            <Button variant="ghost" onClick={() => void pauseHour().then(setSnap)}>
              暂停 1 小时
            </Button>
            <Button variant="ghost" onClick={() => void pauseToday().then(setSnap)}>
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
          <div className="update-modal-card">
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
    </main>
  );
}
