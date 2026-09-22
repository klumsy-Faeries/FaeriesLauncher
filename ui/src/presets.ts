// Turns a preset install outcome into user-facing toasts. Three places can
// start an install (setup wizard, Instances, Mods); they must report alike.

import { t } from "./i18n";
import type { PresetOutcome } from "./ipc/types";
import { pushToast } from "./toasts";

export function reportPreset(outcome: PresetOutcome) {
  const loader = outcome.loaderInstalled
    ? t("presets.loaderInstalled", { version: outcome.loaderInstalled })
    : "";
  const packs =
    outcome.packs.length > 0
      ? t("presets.installedPacks", { count: outcome.packs.length, names: outcome.packs.join(", ") })
      : "";
  pushToast(t("presets.installed", { count: outcome.installed.length, loader, packs }), "info");
  if (outcome.removed.length > 0) {
    pushToast(
      t("presets.removed", { count: outcome.removed.length, names: outcome.removed.join(", ") }),
      "info",
    );
  }
  if (outcome.skipped.length > 0) {
    pushToast(
      t("presets.skipped", {
        count: outcome.skipped.length,
        names: outcome.skipped.map((s) => s.name).join(", "),
      }),
      "warn",
    );
  }
}
