// Keyboard shortcuts (§22).
//
// Bindings come from the `shortcuts.*` settings, so they are validated,
// persisted, and editable in Settings like any other setting. Accelerators
// are written the way users expect to read them: `Ctrl+Shift+R`, `Ctrl+,`.

export interface Accelerator {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  /** Lower-cased key name, e.g. `k`, `,`, `f5`. */
  key: string;
}

/** Parse `"Ctrl+Shift+R"`. Returns `null` for anything unusable. */
export function parseAccelerator(text: string): Accelerator | null {
  const raw = text.trim();
  if (!raw) return null;

  const parts = raw.split("+").map((p) => p.trim()).filter(Boolean);
  // A trailing `+` means the key itself is `+` (e.g. "Ctrl++").
  if (raw.endsWith("+") && !parts.includes("+")) parts.push("+");
  if (parts.length === 0) return null;

  const accel: Accelerator = { ctrl: false, shift: false, alt: false, key: "" };
  for (const part of parts) {
    const lower = part.toLowerCase();
    switch (lower) {
      case "ctrl":
      case "control":
      case "cmd":
      case "meta":
        accel.ctrl = true;
        break;
      case "shift":
        accel.shift = true;
        break;
      case "alt":
      case "option":
        accel.alt = true;
        break;
      default:
        accel.key = lower;
    }
  }
  return accel.key ? accel : null;
}

/** Does this keyboard event match the accelerator? */
export function matches(event: KeyboardEvent, accel: Accelerator): boolean {
  // Ctrl and Cmd are treated alike so one binding works on every platform.
  const ctrl = event.ctrlKey || event.metaKey;
  return (
    ctrl === accel.ctrl &&
    event.shiftKey === accel.shift &&
    event.altKey === accel.alt &&
    event.key.toLowerCase() === accel.key
  );
}

/** Render an accelerator back to display form. */
export function formatAccelerator(accel: Accelerator): string {
  const parts: string[] = [];
  if (accel.ctrl) parts.push("Ctrl");
  if (accel.shift) parts.push("Shift");
  if (accel.alt) parts.push("Alt");
  parts.push(accel.key.length === 1 ? accel.key.toUpperCase() : accel.key);
  return parts.join("+");
}

/** True when the event target is somewhere the user is typing. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable
  );
}

export interface Binding {
  /** The `shortcuts.*` setting id this came from. */
  settingId: string;
  accel: Accelerator;
  run: () => void;
}

/**
 * Install a global key listener for `bindings`. Returns a disposer.
 *
 * Shortcuts are ignored while the user is typing, except for ones that
 * include a modifier — `Ctrl+K` should still open the palette from inside a
 * text field, but a bare letter must not.
 */
export function installShortcuts(bindings: () => Binding[]): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    for (const binding of bindings()) {
      if (!matches(event, binding.accel)) continue;
      const hasModifier =
        binding.accel.ctrl || binding.accel.alt || binding.accel.shift;
      if (!hasModifier && isTypingTarget(event.target)) continue;
      event.preventDefault();
      binding.run();
      return;
    }
  };
  window.addEventListener("keydown", onKeyDown);
  return () => window.removeEventListener("keydown", onKeyDown);
}
