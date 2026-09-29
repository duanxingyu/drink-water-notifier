/** 提醒窗默认右下角，并记住拖拽后的位置（不做贴边吸附）。 */

import {
  currentMonitor,
  getCurrentWindow,
  PhysicalPosition,
  primaryMonitor,
  type Monitor,
} from "@tauri-apps/api/window";

type PosState = {
  x: number;
  y: number;
};

const STORAGE_KEY = "rundi.reminder.pos";
const MARGIN_PX = 16;

function loadPos(): PosState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PosState;
    if (typeof parsed.x !== "number" || typeof parsed.y !== "number") return null;
    return parsed;
  } catch {
    return null;
  }
}

function savePos(pos: PosState) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(pos));
}

async function activeMonitor(): Promise<Monitor | null> {
  return (await currentMonitor()) ?? (await primaryMonitor());
}

export async function placeBottomRight(): Promise<void> {
  const win = getCurrentWindow();
  const monitor = await activeMonitor();
  if (!monitor) return;
  const size = await win.outerSize();
  const scale = monitor.scaleFactor;
  const work = monitor.workArea;
  const margin = Math.round(MARGIN_PX * scale);
  const x = Math.max(work.position.x, work.position.x + work.size.width - size.width - margin);
  const y = Math.max(work.position.y, work.position.y + work.size.height - size.height - margin);
  await win.setPosition(new PhysicalPosition(x, y));
  savePos({ x, y });
}

export async function restorePosition(): Promise<void> {
  const saved = loadPos();
  if (!saved) {
    await placeBottomRight();
    return;
  }
  const win = getCurrentWindow();
  const monitor = await activeMonitor();
  if (!monitor) {
    await win.setPosition(new PhysicalPosition(saved.x, saved.y));
    return;
  }
  const size = await win.outerSize();
  const work = monitor.workArea;
  const x = Math.min(
    Math.max(saved.x, work.position.x),
    work.position.x + work.size.width - size.width,
  );
  const y = Math.min(
    Math.max(saved.y, work.position.y),
    work.position.y + work.size.height - size.height,
  );
  await win.setPosition(new PhysicalPosition(x, y));
}

export async function rememberPosition(): Promise<void> {
  const win = getCurrentWindow();
  const pos = await win.outerPosition();
  savePos({ x: pos.x, y: pos.y });
}
