// Task activity feed for the Downloads page.
//
// Purely event-driven (§7: no polling). The backend's TaskScheduler emits
// started/progress/finished; this keeps a bounded record of them so the user
// can see what the launcher is doing and what it recently did.

import { createSignal } from "solid-js";

import type { FaerieEvent } from "./ipc/types";

export interface TaskRecord {
  id: number;
  name: string;
  /** 0..1, or null before the first progress report. */
  progress: number | null;
  message: string | null;
  state: "running" | "completed" | "cancelled" | "failed";
  error?: string;
}

/** Finished tasks are kept for context, but never without bound. */
const MAX_FINISHED = 50;

export const [tasks, setTasks] = createSignal<TaskRecord[]>([]);

export function activeTasks(): TaskRecord[] {
  return tasks().filter((task) => task.state === "running");
}

export function clearFinishedTasks() {
  setTasks((list) => list.filter((task) => task.state === "running"));
}

/** Route a task event into the feed. Returns true if it was handled. */
export function handleTaskEvent(event: FaerieEvent): boolean {
  switch (event.type) {
    case "taskStarted":
      setTasks((list) => [
        { id: event.id, name: event.name, progress: null, message: null, state: "running" },
        ...list,
      ]);
      return true;

    case "taskProgress":
      setTasks((list) =>
        list.map((task) =>
          task.id === event.id
            ? { ...task, progress: event.progress, message: event.message }
            : task,
        ),
      );
      return true;

    case "taskFinished": {
      const outcome = event.outcome;
      setTasks((list) => {
        const updated = list.map((task): TaskRecord =>
          task.id === event.id
            ? {
                ...task,
                state:
                  outcome.kind === "completed"
                    ? "completed"
                    : outcome.kind === "cancelled"
                      ? "cancelled"
                      : "failed",
                error: outcome.kind === "failed" ? outcome.error : undefined,
                progress: outcome.kind === "completed" ? 1 : task.progress,
              }
            : task,
        );
        // Trim the finished tail, keeping every running task.
        const running = updated.filter((t) => t.state === "running");
        const finished = updated
          .filter((t) => t.state !== "running")
          .slice(0, MAX_FINISHED);
        return [...running, ...finished];
      });
      return true;
    }

    default:
      return false;
  }
}
