import { lazy, Suspense, useEffect, useState } from "react";

import { isTauri } from "@/api";

type View = "settings" | "reminder";

const Settings = lazy(async () => {
  const mod = await import("@/Settings");
  return { default: mod.Settings };
});

const Reminder = lazy(async () => {
  const mod = await import("@/Reminder");
  return { default: mod.Reminder };
});

function initialView(): View | null {
  const query = new URLSearchParams(window.location.search).get("view");
  if (query === "reminder" || query === "settings") return query;
  // 桌面窗口要等标签回来再选页面，避免提醒窗先闪出设置。
  return isTauri() ? null : "settings";
}

export function App() {
  const [view, setView] = useState<View | null>(initialView);

  useEffect(() => {
    document.documentElement.classList.toggle("reminder-view", view === "reminder");
    document.documentElement.classList.toggle("settings-view", view === "settings");
    document.documentElement.classList.toggle("preview", !isTauri() && view !== null);
  }, [view]);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    let stop = false;
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      if (stop) return;
      setView(getCurrentWindow().label === "reminder" ? "reminder" : "settings");
    });
    return () => {
      stop = true;
    };
  }, []);

  if (view === null) return <div className="stage" />;
  return (
    <Suspense fallback={<div className="stage" />}>
      {view === "reminder" ? <Reminder /> : <Settings />}
    </Suspense>
  );
}
