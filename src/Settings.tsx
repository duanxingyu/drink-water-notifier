import { useEffect, useState } from "react";

import {
  defaultAmountFor,
  getSnapshot,
  onSnapshot,
  pauseHour,
  pauseToday,
  remindNow,
  resume,
  saveSettings,
  type DrinkUnit,
  type SettingsInput,
  type Snapshot,
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

export function Settings() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [form, setForm] = useState<SettingsInput | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  async function load() {
    setLoading(true);
    try {
      const next = await getSnapshot();
      setSnap(next);
      setForm(toForm(next));
      setError(next.configWarning);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let unlisten = () => {};
    let stop = false;
    void (async () => {
      unlisten = await onSnapshot((next) => {
        if (stop) return;
        setSnap(next);
        setError(next.configWarning);
      });
      if (!stop) await load();
    })();
    return () => {
      stop = true;
      unlisten();
    };
  }, []);

  function patch(partial: Partial<SettingsInput>) {
    setSaved(false);
    setForm((current) => (current ? { ...current, ...partial } : current));
  }

  function toggleDay(code: string) {
    if (!form) return;
    const has = form.workdays.includes(code);
    const workdays = has ? form.workdays.filter((day) => day !== code) : [...form.workdays, code];
    patch({ workdays });
  }

  function setUnit(unit: DrinkUnit) {
    if (!form) return;
    const amount =
      form.drinkUnit === unit ? form.drinkAmount : defaultAmountFor(unit);
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
    try {
      const next = await saveSettings(form);
      setSnap(next);
      setForm(toForm(next));
      setError(next.configWarning);
      setSaved(!next.configWarning);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
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
            step={form.drinkUnit === "ml" ? 10 : 1}
            value={form.drinkAmount}
            onChange={(event) => patch({ drinkAmount: Number(event.target.value) })}
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
            value={form.intervalMinutes}
            onChange={(event) => patch({ intervalMinutes: Number(event.target.value) })}
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
              value={form.autoDismissSeconds}
              onChange={(event) => patch({ autoDismissSeconds: Number(event.target.value) })}
            />
          </div>
          <div className="field">
            <Label htmlFor="snooze">稍后提醒（分钟）</Label>
            <Input
              id="snooze"
              type="number"
              min={1}
              max={60}
              value={form.snoozeMinutes}
              onChange={(event) => patch({ snoozeMinutes: Number(event.target.value) })}
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
            <p className="hint">登录后在后台运行。请在安装好的应用里打开，开发版路径会变。</p>
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
          <div className="row">
            <Button variant="ghost" onClick={() => void pauseHour().then(setSnap)}>
              暂停 1 小时
            </Button>
            <Button variant="ghost" onClick={() => void pauseToday().then(setSnap)}>
              今日暂停
            </Button>
          </div>
        </div>

        <p className="path">配置文件 {snap.configPath}</p>
      </div>
    </main>
  );
}
