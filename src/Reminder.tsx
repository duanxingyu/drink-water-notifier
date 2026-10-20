import { useEffect, useRef, useState } from "react";

import {
  activeReminder,
  counterpartUnit,
  dismiss,
  drink,
  formatIntake,
  formatIntakeApprox,
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
    // 托盘/快捷键记水：直接显示确认 toast，不进「该喝水啦」流程。
    setPhase(next.kind === "ack" ? "drank" : "ask");
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
    if (!payload || exiting) return;
    const tickingAsk = phase === "ask";
    const tickingAck = phase === "drank" && payload.kind === "ack";
    if (!tickingAsk && !tickingAck) return;
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
      if (exiting || !payload) return;
      if (event.key === "Escape") {
        if (phase === "ask" || payload.kind === "ack") {
          void exitThen(() => {
            setPhase("gone");
            void dismiss();
          });
        }
        return;
      }
      if (phase !== "ask") return;
      if (event.key === "Enter" && payload.kind !== "summary") {
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

  async function confirmSummary() {
    if (exiting) return;
    exitThen(() => {
      setPhase("gone");
      void dismiss();
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
  const isSummary = payload.kind === "summary";
  const isAck = payload.kind === "ack";
  const cardClass = [
    exiting ? "card card-out" : "card card-in",
    isSummary ? "card-summary" : "",
    isAck ? "card-ack" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const dialogLabel = isSummary ? "今日小结" : isAck ? "已记下" : "该喝水啦";
  const goalProgress =
    payload.deskGoalMl > 0
      ? Math.min(1, payload.intakeMlToday / payload.deskGoalMl)
      : 0;
  const goalPct = Math.round(goalProgress * 100);
  const goalReached = payload.deskGoalMl > 0 && payload.intakeMlToday >= payload.deskGoalMl;
  const goalRemaining = Math.max(0, payload.deskGoalMl - payload.intakeMlToday);
  const intakeApprox = formatIntakeApprox(
    payload.glassesToday,
    payload.drinkUnit,
    counterpartUnit(payload.drinkUnit),
  );
  const todayLine = new Date().toLocaleDateString("zh-CN", {
    month: "long",
    day: "numeric",
    weekday: "short",
  });

  return (
    <div className="stage">
      <section
        key={payload.token}
        className={cardClass}
        role="dialog"
        aria-label={dialogLabel}
        onMouseDown={(event) => void onDragDown(event)}
      >
        <div className="brand card-item">
          <span>润滴</span>
          <span className="drag-hint">拖动 · {left}s</span>
        </div>
        {phase === "ask" ? (
          isSummary ? (
            <>
              <div className="summary-head card-item">
                <p className="summary-kicker">今日小结</p>
                <p className="summary-date">{todayLine}</p>
              </div>
              <div className="summary-hero card-item">
                <p className="summary-caption">工位已喝</p>
                <p className="summary-amount">{payload.intakeLabel}</p>
                {intakeApprox ? (
                  <p className="summary-approx">{intakeApprox}</p>
                ) : payload.intakeMlToday > 0 && payload.drinkUnit !== "ml" ? (
                  <p className="summary-approx">约 {payload.intakeMlToday} 毫升</p>
                ) : null}
              </div>
              {payload.highIntakeMode && payload.deskGoalMl > 0 ? (
                <div className="summary-goal card-item" aria-label="工位目标进度">
                  <div className="summary-goal-row">
                    <span>工位目标</span>
                    <span>
                      {payload.intakeMlToday}/{payload.deskGoalMl} 毫升
                    </span>
                  </div>
                  <div
                    className="summary-goal-bar"
                    role="progressbar"
                    aria-valuenow={goalPct}
                    aria-valuemin={0}
                    aria-valuemax={100}
                  >
                    <div
                      className="summary-goal-fill"
                      style={{ width: `${goalPct}%` }}
                    />
                  </div>
                  <p className="summary-goal-note">
                    {goalReached
                      ? "今日工位目标已达成"
                      : `还差 ${goalRemaining} 毫升`}
                  </p>
                </div>
              ) : (
                <div className="summary-stat card-item">
                  <span className="summary-stat-label">今日毫升</span>
                  <span className="summary-stat-value">{payload.intakeMlToday} ml</span>
                </div>
              )}
              <p className="summary-note card-item">{payload.drinkPrompt}</p>
              <div className="actions card-item">
                <Button variant="drink" size="default" onClick={() => void confirmSummary()}>
                  知道了
                </Button>
              </div>
            </>
          ) : (
            <>
              <div className="summary-head card-item">
                <h1 className="summary-kicker">该喝水啦</h1>
              </div>
              <p className="drink-note card-item">{payload.drinkPrompt}</p>
              <div className="glass-wrap card-item">
                <Glass dismissSeconds={payload.autoDismissSeconds} />
              </div>
              <div className="drink-stats card-item">
                <div className="summary-stat">
                  <span className="summary-stat-label">今日已喝</span>
                  <span className="summary-stat-value">{payload.intakeLabel}</span>
                </div>
                {payload.highIntakeMode && payload.deskGoalMl > 0 ? (
                  <div className="summary-goal" aria-label="工位目标进度">
                    <div className="summary-goal-row">
                      <span>工位目标</span>
                      <span>
                        {payload.intakeMlToday}/{payload.deskGoalMl} 毫升
                      </span>
                    </div>
                    <div
                      className="summary-goal-bar"
                      role="progressbar"
                      aria-valuenow={goalPct}
                      aria-valuemin={0}
                      aria-valuemax={100}
                    >
                      <div
                        className="summary-goal-fill"
                        style={{ width: `${goalPct}%` }}
                      />
                    </div>
                  </div>
                ) : intakeApprox ? (
                  <p className="drink-approx">{intakeApprox}</p>
                ) : null}
              </div>
              <div className="actions card-item">
                <Button variant="drink" size="default" onClick={() => void confirmDrink()}>
                  喝了
                </Button>
                <Button variant="outline" size="default" onClick={() => void confirmLater()}>
                  稍后（{minutes} 分）
                </Button>
              </div>
            </>
          )
        ) : isAck ? (
          <>
            <div className="summary-head card-item">
              <p className="summary-kicker">已记下</p>
              <p className="summary-date">
                +{formatIntake(payload.drinkAmount, payload.drinkUnit)}
              </p>
            </div>
            <div className="summary-hero card-item">
              <p className="summary-caption">今日合计</p>
              <p className="summary-amount">{payload.intakeLabel}</p>
              {intakeApprox ? (
                <p className="summary-approx">{intakeApprox}</p>
              ) : payload.intakeMlToday > 0 ? (
                <p className="summary-approx">{payload.intakeMlToday} 毫升</p>
              ) : null}
            </div>
            {payload.highIntakeMode && payload.deskGoalMl > 0 ? (
              <div className="summary-goal card-item" aria-label="工位目标进度">
                <div className="summary-goal-row">
                  <span>工位目标</span>
                  <span>
                    {goalReached ? "已达成" : `还差 ${goalRemaining} 毫升`}
                  </span>
                </div>
                <div
                  className="summary-goal-bar"
                  role="progressbar"
                  aria-valuenow={goalPct}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="summary-goal-fill"
                    style={{ width: `${goalPct}%` }}
                  />
                </div>
              </div>
            ) : (
              <p className="ack-hint card-item">{payload.drinkAck}</p>
            )}
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
