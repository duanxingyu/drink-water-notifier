import { useEffect, useState } from "react";

import { isTauri } from "@/api";
import { Reminder } from "@/Reminder";
import { Settings } from "@/Settings";

type View = "settings" | "reminder";

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
  return view === "reminder" ? <Reminder /> : <Settings />;
}
