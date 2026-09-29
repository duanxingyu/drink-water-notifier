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
import { placeBottomRight, rememberPosition, restorePosition } from "@/windowDock";

type Phase = "ask" | "drank" | "later" | "gone";

const EXIT_MS = 380;

export function Reminder() {
  const [payload, setPayload] = useState<ReminderPayload | null>(null);
  const [left, setLeft] = useState(0);
  const [phase, setPhase] = useState<Phase>("ask");
  const [exiting, setExiting] = useState(false);
  const seen = useRef(0);
  const exitingRef = useRef(false);

  function arm(next: ReminderPayload) {
    if (next.token <= seen.current) return;
    seen.current = next.token;
    exitingRef.current = false;
    setExiting(false);
    setPayload(next);
    setLeft(next.autoDismissSeconds);
    setPhase("ask");
    if (isTauri()) {
      void restorePosition().catch(() => void placeBottomRight());
    }
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
    if (!payload || phase !== "ask" || exiting) return;
    const started = Date.now();
    const total = payload.autoDismissSeconds * 1000;
    const timer = window.setInterval(() => {
      const remain = Math.max(0, total - (Date.now() - started));
      setLeft(Math.ceil(remain / 1000));
      if (remain <= 0) {
        window.clearInterval(timer);
        void exitThen(() => {
          setPhase("gone");
          void dismiss();
        });
      }
    }, 200);
    return () => window.clearInterval(timer);
  }, [payload, phase, exiting]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (phase !== "ask" || exiting) return;
      if (event.key === "Escape") {
        void exitThen(() => {
          setPhase("gone");
          void dismiss();
        });
      }
      if (event.key === "Enter") {
        void confirmDrink();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function exitThen(done: () => void) {
    if (exitingRef.current) return;
    exitingRef.current = true;
    setExiting(true);
    window.setTimeout(done, EXIT_MS);
  }

  async function confirmDrink() {
    if (exiting) return;
    setPhase("drank");
    exitThen(() => {
      void drink();
    });
  }

  async function confirmLater() {
    if (exiting) return;
    setPhase("later");
    exitThen(() => {
      void snooze();
    });
  }

  async function onDragDown(event: React.MouseEvent) {
    if (!isTauri() || event.button !== 0 || exiting) return;
    if ((event.target as HTMLElement).closest("button, a, input")) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().startDragging();
    } finally {
      await rememberPosition();
    }
  }

  if (!payload || phase === "gone") {
    return <div className="stage" />;
  }

  const minutes = payload.snoozeMinutes;
  const cardClass = exiting ? "card card-out" : "card card-in";

  return (
    <div className="stage">
      <section
        key={payload.token}
        className={cardClass}
        role="dialog"
        aria-label="该喝水啦"
        onMouseDown={(event) => void onDragDown(event)}
      >
        <div className="brand card-item">
          <span>润滴</span>
          <span className="drag-hint">拖动 · {left}s</span>
        </div>
        {phase === "ask" ? (
          <>
            <h1 className="headline card-item">该喝水啦</h1>
            <p className="sub card-item">{payload.drinkPrompt}</p>
            <div className="glass-wrap card-item">
              <Glass dismissSeconds={payload.autoDismissSeconds} />
            </div>
            <p className="meta card-item">今日已喝 {payload.intakeLabel}</p>
            <div className="actions card-item">
              <Button variant="drink" size="default" onClick={() => void confirmDrink()}>
                喝了
              </Button>
              <Button variant="outline" size="default" onClick={() => void confirmLater()}>
                稍后（{minutes} 分）
              </Button>
            </div>
          </>
        ) : (
          <p className="toast card-item">
            {phase === "drank" ? payload.drinkAck : `${minutes} 分钟后再叫你`}
          </p>
        )}
      </section>
    </div>
  );
}
