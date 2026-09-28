import { useEffect, useRef, useState } from "react";

import {
  activeReminder,
  dismiss,
  drink,
  isTauri,
  onReminder,
  previewReminder,
  snooze,
  type ReminderPayload,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Glass } from "@/Glass";

type Phase = "ask" | "drank" | "later" | "gone";

export function Reminder() {
  const [payload, setPayload] = useState<ReminderPayload | null>(null);
  const [left, setLeft] = useState(0);
  const [phase, setPhase] = useState<Phase>("ask");
  const seen = useRef(0);

  function arm(next: ReminderPayload) {
    if (next.token <= seen.current) return;
    seen.current = next.token;
    setPayload(next);
    setLeft(next.autoDismissSeconds);
    setPhase("ask");
  }

  useEffect(() => {
    let stop = false;
    let unlisten = () => {};
    if (!isTauri()) {
      arm(previewReminder());
      return;
    }
    void (async () => {
      unlisten = await onReminder((next) => {
        if (!stop) arm(next);
      });
      const active = await activeReminder();
      if (!stop && active) arm(active);
    })();
    return () => {
      stop = true;
      unlisten();
    };
  }, []);

  useEffect(() => {
    if (!payload || phase !== "ask") return;
    const started = Date.now();
    const total = payload.autoDismissSeconds * 1000;
    const timer = window.setInterval(() => {
      const remain = Math.max(0, total - (Date.now() - started));
      setLeft(Math.ceil(remain / 1000));
      if (remain <= 0) {
        window.clearInterval(timer);
        setPhase("gone");
        void dismiss();
      }
    }, 200);
    return () => window.clearInterval(timer);
  }, [payload, phase]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (phase !== "ask") return;
      if (event.key === "Escape") {
        setPhase("gone");
        void dismiss();
      }
      if (event.key === "Enter") {
        void confirmDrink();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  async function confirmDrink() {
    setPhase("drank");
    window.setTimeout(() => {
      void drink();
    }, 420);
  }

  async function confirmLater() {
    setPhase("later");
    window.setTimeout(() => {
      void snooze();
    }, 420);
  }

  if (!payload || phase === "gone") {
    return <div className="stage" />;
  }

  const minutes = payload.snoozeMinutes;

  return (
    <div
      className="stage"
      onClick={() => {
        setPhase("gone");
        void dismiss();
      }}
    >
      <section
        className="card"
        role="dialog"
        aria-label="该喝水啦"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="brand">
          <span>润滴</span>
          <span>{left} 秒后关闭</span>
        </div>
        {phase === "ask" ? (
          <>
            <h1 className="headline">该喝水啦 💧</h1>
            <p className="sub">离开屏幕一小会儿，喝一杯水。</p>
            <div className="glass-wrap">
              <Glass dismissSeconds={payload.autoDismissSeconds} />
            </div>
            <p className="meta">今日已喝 {payload.glassesToday} 杯</p>
            <div className="actions">
              <Button variant="drink" size="lg" onClick={() => void confirmDrink()}>
                喝了
              </Button>
              <Button variant="outline" size="lg" onClick={() => void confirmLater()}>
                稍后提醒（{minutes} 分钟）
              </Button>
            </div>
          </>
        ) : (
          <p className="toast">{phase === "drank" ? "记下这一杯" : `${minutes} 分钟后再叫你`}</p>
        )}
      </section>
    </div>
  );
}
