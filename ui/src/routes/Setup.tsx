// First-launch experience (§37).
//
// Shows once, before the main window, and covers only what a new user
// genuinely needs: what was detected on their machine, how it should look,
// signing in, and a first instance. Advanced options stay in Settings —
// the wizard deliberately does not try to be a second settings page.

import { createResource, createSignal, For, Show } from "solid-js";

import { t } from "../i18n";
import { backend } from "../ipc/backend";
import { reportPreset } from "../presets";
import { themeLabel } from "../theme/apply";
import { pushToast } from "../toasts";

const STEPS = ["welcome", "system", "appearance", "account", "instance"] as const;
type Step = (typeof STEPS)[number];

export function Setup(props: { onFinish: () => void }) {
  const [step, setStep] = createSignal(0);
  const [busy, setBusy] = createSignal(false);

  const [hardware] = createResource(() => backend.detectHardware());
  const [java] = createResource(() => backend.detectJava());
  const [info] = createResource(() => backend.appInfo());
  const [themes] = createResource(() => backend.listThemes());
  const [accounts, { refetch: refetchAccounts }] = createResource(() =>
    backend.listAccounts(),
  );
  const manifestRequest = backend
    .listMinecraftVersions(false)
    .then((result) => result.manifest);
  const [versions] = createResource(() => manifestRequest);

  const [instanceName, setInstanceName] = createSignal("Faeries");
  const [instanceVersion, setInstanceVersion] = createSignal("");
  const [optimized, setOptimized] = createSignal(true);

  const current = (): Step => STEPS[step()] ?? "welcome";
  const isLast = () => step() === STEPS.length - 1;

  const next = () => setStep((s) => Math.min(s + 1, STEPS.length - 1));
  const back = () => setStep((s) => Math.max(s - 1, 0));

  const finish = async () => {
    setBusy(true);
    try {
      // Creating an instance is optional; skip silently if none was named.
      const name = instanceName().trim();
      // Wait for the manifest rather than silently skipping the instance
      // when Finish is pressed before it has arrived.
      const manifest = versions() ?? (await manifestRequest);
      const version = instanceVersion() || manifest?.latest.release;
      if (name && version) {
        const created = await backend.createInstance(name, version);
        if (optimized()) {
          pushToast(t("presets.installing"), "info");
          reportPreset(await backend.installModPreset(created.id, "optimized"));
        }
      }
      await backend.setSetting("launcher.setup_complete", true);
      props.onFinish();
    } catch (error) {
      pushToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  };

  const skip = async () => {
    setBusy(true);
    try {
      await backend.setSetting("launcher.setup_complete", true);
      props.onFinish();
    } catch (error) {
      pushToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="setup-backdrop">
      <div class="setup card">
        <div class="setup-steps">
          <For each={STEPS}>
            {(id, index) => (
              <span
                class={`setup-dot${index() === step() ? " active" : ""}${
                  index() < step() ? " done" : ""
                }`}
                title={t(`setup.step.${id}`)}
              />
            )}
          </For>
        </div>

        <h1 class="setup-title">{t(`setup.${current()}.title`)}</h1>
        <p class="muted">{t(`setup.${current()}.body`)}</p>

        <div class="setup-body">
          <Show when={current() === "welcome"}>
            <Show when={info()}>
              {(i) => (
                <div class="mini-list">
                  <div class="mini-row">
                    <span class="muted">{t("settings.aboutVersion")}</span>
                    <span class="mini-name">{i().version}</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("settings.aboutDataDir")}</span>
                    <code>{i().dataDir}</code>
                  </div>
                </div>
              )}
            </Show>
          </Show>

          <Show when={current() === "system"}>
            <Show when={hardware()} fallback={<p class="muted">{t("home.card.detecting")}</p>}>
              {(hw) => (
                <div class="mini-list">
                  <div class="mini-row">
                    <span class="muted">{t("home.card.cpu")}</span>
                    <span>{hw().cpuModel}</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("home.card.ram")}</span>
                    <span>{Math.round(hw().totalRamMb / 1024)} GB</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("home.card.suggested")}</span>
                    <span>{hw().recommendedHeapMb} MB</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("home.card.java")}</span>
                    <span>
                      <Show
                        when={(java()?.length ?? 0) > 0}
                        fallback={t("setup.system.noJava")}
                      >
                        {java()!.map((j) => `Java ${j.major}`).join(", ")}
                      </Show>
                    </span>
                  </div>
                </div>
              )}
            </Show>
          </Show>

          <Show when={current() === "appearance"}>
            <div class="setup-field">
              <label for="setup-theme">{t("setting.ui.theme.name")}</label>
              <select
                id="setup-theme"
                onChange={(e) =>
                  void backend.setSetting("ui.theme", e.currentTarget.value)
                }
              >
                <For each={themes() ?? []}>
                  {(name) => <option value={name}>{themeLabel(name)}</option>}
                </For>
              </select>
            </div>
          </Show>

          <Show when={current() === "account"}>
            <Show
              when={accounts()?.signInAvailable}
              fallback={<p class="muted">{t("accounts.needClientId")}</p>}
            >
              <Show
                when={(accounts()?.accounts.length ?? 0) === 0}
                fallback={
                  <p class="mini-name">
                    {t("home.signedInAs", {
                      name: accounts()!.accounts[0]!.name,
                    })}
                  </p>
                }
              >
                <button
                  type="button"
                  class="button-primary"
                  disabled={busy()}
                  onClick={async () => {
                    setBusy(true);
                    try {
                      await backend.beginSignIn();
                      await backend.completeSignIn();
                      await refetchAccounts();
                    } catch (error) {
                      pushToast(String(error), "error");
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  {t("accounts.signIn")}
                </button>
              </Show>
            </Show>
          </Show>

          <Show when={current() === "instance"}>
            <div class="setup-field">
              <label for="setup-name">{t("home.card.name")}</label>
              <input
                id="setup-name"
                type="text"
                value={instanceName()}
                onInput={(e) => setInstanceName(e.currentTarget.value)}
              />
            </div>
            <div class="setup-field">
              <label for="setup-version">{t("home.card.version")}</label>
              <select
                id="setup-version"
                value={instanceVersion() || versions()?.latest.release || ""}
                onChange={(e) => setInstanceVersion(e.currentTarget.value)}
              >
                <For
                  each={(versions()?.versions ?? [])
                    .filter((v) => v.kind === "release")
                    .slice(0, 40)}
                >
                  {(v) => <option value={v.id}>{v.id}</option>}
                </For>
              </select>
            </div>
            <label class="checkbox-row setup-field" title={t("presets.optimized.hint")}>
              <input
                type="checkbox"
                checked={optimized()}
                onChange={(e) => setOptimized(e.currentTarget.checked)}
              />
              {t("presets.startOptimized")}
            </label>
          </Show>
        </div>

        <div class="setup-actions">
          <button type="button" onClick={skip} disabled={busy()}>
            {t("setup.skip")}
          </button>
          <div class="setup-nav">
            <Show when={step() > 0}>
              <button type="button" onClick={back} disabled={busy()}>
                {t("setup.back")}
              </button>
            </Show>
            <Show
              when={!isLast()}
              fallback={
                <button
                  type="button"
                  class="button-primary"
                  onClick={finish}
                  disabled={busy()}
                >
                  {t("setup.finish")}
                </button>
              }
            >
              <button type="button" class="button-primary" onClick={next}>
                {t("setup.next")}
              </button>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
}
