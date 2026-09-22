// Play-flow state: install progress, console output, and the running game.
// Fed entirely by backend events (§7: no polling).

import { createSignal } from "solid-js";

import type { FaerieEvent, RunningGame } from "./ipc/types";

export interface InstallStatus {
  phase: string;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number | null;
  bytesPerSec: number;
}

export interface ConsoleLine {
  id: number;
  stderr: boolean;
  text: string;
}

/** Bounded console buffer — a spammy game can never grow memory (§26). */
const MAX_CONSOLE_LINES = 2000;

export const [installStatus, setInstallStatus] = createSignal<InstallStatus | null>(null);
export const [running, setRunning] = createSignal<RunningGame | null>(null);
export const [consoleLines, setConsoleLines] = createSignal<ConsoleLine[]>([]);
export const [lastExit, setLastExit] = createSignal<{
  class: string;
  detail: string;
  code: number | null;
} | null>(null);

let nextLineId = 1;

export function clearConsole() {
  setConsoleLines([]);
}

/** Fraction 0..1 for the current install, or null when size is unknown. */
export function installFraction(status: InstallStatus): number | null {
  if (status.bytesTotal && status.bytesTotal > 0) {
    return Math.min(1, status.bytesDone / status.bytesTotal);
  }
  if (status.filesTotal > 0) return Math.min(1, status.filesDone / status.filesTotal);
  return null;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${bytes} B`;
}

/** Route a backend event into play state. Returns true if it was handled. */
export function handlePlayEvent(event: FaerieEvent): boolean {
  switch (event.type) {
    case "installProgress":
      setInstallStatus({
        phase: event.phase,
        filesDone: event.filesDone,
        filesTotal: event.filesTotal,
        bytesDone: event.bytesDone,
        bytesTotal: event.bytesTotal,
        bytesPerSec: event.bytesPerSec,
      });
      return true;
    case "gameStarted":
      setInstallStatus(null);
      setLastExit(null);
      clearConsole();
      setRunning({ instanceId: event.instanceId, pid: event.pid });
      return true;
    case "gameLog":
      setConsoleLines((lines) => {
        const next = [...lines, { id: nextLineId++, stderr: event.stderr, text: event.text }];
        return next.length > MAX_CONSOLE_LINES
          ? next.slice(next.length - MAX_CONSOLE_LINES)
          : next;
      });
      return true;
    case "gameExited":
      setRunning(null);
      setInstallStatus(null);
      setLastExit({ class: event.class, detail: event.detail, code: event.code });
      return true;
    default:
      return false;
  }
}
