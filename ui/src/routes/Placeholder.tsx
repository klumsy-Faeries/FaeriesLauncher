import type { Component } from "solid-js";

import { t } from "../i18n";

// Placeholder pages for subsystems arriving in later phases. Each names its
// phase so the skeleton is honest about what exists.
export function placeholderPage(id: string): Component {
  return () => (
    <section class="page">
      <h1>{t(`nav.${id}`)}</h1>
      <p class="muted">{t(`page.${id}.text`)}</p>
    </section>
  );
}
