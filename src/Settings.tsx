import { useEffect, useState } from "react";

import {
  getSnapshot,
  onSnapshot,
  pauseHour,
  pauseToday,
  remindNow,
  resume,
  saveSettings,
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

  async function onSave() {
    if (!form) return;
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
          <p className="lede">工作日里，到点就轻轻提醒你喝水。关掉这个窗口后，它还在托盘里。</p>
        </header>

        <section className="count-block" aria-label="今日杯数">
          <div>
            <div className="count-num">{String(snap.glassesToday).padStart(2, "0")}</div>
            <div className="count-caption">今日已喝，单位是杯</div>
          </div>
          <p className="hint">
            {snap.glassesToday === 0 ? "今天还没有记录。点「喝了」才会计数。" : "点「喝了」才会加一杯。"}
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
        {saved ? <p className="hint">已保存。</p> : null}

        <div className="actions">
          <Button onClick={() => void onSave()} disabled={saving}>
            {saving ? "正在保存…" : "保存设置"}
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
