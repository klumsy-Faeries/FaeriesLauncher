import { A } from "@solidjs/router";
import { createEffect, createResource, createSignal, For, Show } from "solid-js";

import { Console } from "../components/Console";
import { t } from "../i18n";
import { backend } from "../ipc/backend";
import type { Instance } from "../ipc/types";
import { themeAsset } from "../theme/apply";
import {
  formatBytes,
  installFraction,
  installStatus,
  lastExit,
  running,
  setRunning,
} from "../play";
import { theme } from "../state";
import { pushToast } from "../toasts";

/**
 * The home dashboard. Regions come from `layout.json`'s `home` block (§6),
 * and every panel is filled from real launcher state.
 *
 * Some panels in the visual design correspond to features this launcher
 * does not have (cosmetics, a friends list, a store). Those render an
 * honest empty state rather than invented content, so the layout matches
 * without the interface lying about what exists.
 */
export function Home() {
  const home = () => theme()?.layout.home ?? {};
  const showPlay = () => home().showPlayButton ?? true;
  const showAccount = () => home().showAccountCard ?? true;
  const showNews = () => home().showNews ?? true;
  const cards = () => home().cards ?? ["mods", "instance", "system", "versions"];

  const [instances] = createResource(() => backend.listInstances());
  const [accounts] = createResource(() => backend.listAccounts());
  // "Offline session" must mean no account, never a failed read.
  createEffect(() => {
    if (accounts.error) pushToast(String(accounts.error), "error");
  });
  const [selected, setSelected] = createSignal<string>("");
  const [starting, setStarting] = createSignal(false);

  createResource(async () => {
    const game = await backend.runningGame();
    if (game) setRunning(game);
    return game;
  });

  const list = () => instances()?.instances ?? [];
  const currentId = () => selected() || list()[0]?.id || "";
  const current = () => list().find((i) => i.id === currentId()) ?? null;
  const activeAccountName = () => {
    const info = accounts();
    return info?.accounts.find((a) => a.id === info.active)?.name;
  };

  const play = async () => {
    const id = currentId();
    if (!id) {
      pushToast(t("home.noInstances"), "warn");
      return;
    }
    setStarting(true);
    try {
      await backend.playInstance(id);
    } catch (error) {
      pushToast(String(error), "error");
    } finally {
      setStarting(false);
    }
  };

  const stop = async () => {
    try {
      await backend.stopGame();
    } catch (error) {
      pushToast(String(error), "error");
    }
  };

  const busy = () =>
    installStatus() !== null || running() !== null || lastExit() !== null;

  return (
    <section class="home-layout">
      <Show when={showAccount()}>
        <div class="home-account card">
          <div class="avatar" aria-hidden="true">
            {(activeAccountName() ?? "?").slice(0, 1).toUpperCase()}
          </div>
          <div class="account-lines">
            <span class="account-name">
              {activeAccountName() ?? t("home.offlineAccount")}
              <Show when={activeAccountName()}>
                <span class="verified" title={t("home.premium")}>
                  ✓
                </span>
              </Show>
            </span>
            <span class="muted">
              {activeAccountName() ? t("home.premium") : t("home.offlineMode")}
            </span>
          </div>
          <A href="/accounts" class="card-link">
            {t("home.manageAccount")}
          </A>
        </div>
      </Show>

      <div class="home-status-card card">
        <Show
          when={current()}
          fallback={<span class="muted">{t("home.noInstances")}</span>}
        >
          {(instance) => (
            <>
              <div class="status-block">
                <span class="status-title">{instance().name}</span>
                <span class="muted">
                  {t("home.instanceSummary", {
                    version: instance().minecraftVersion,
                    loader: instance().loader?.kind ?? t("home.card.vanilla"),
                  })}
                </span>
              </div>
              <div class="status-metric">
                <span class="metric-value">{list().length}</span>
                <span class="muted">{t("home.instanceCount")}</span>
              </div>
            </>
          )}
        </Show>
      </div>

      <div class="home-hero">
        <div class="hero-logo">
          <HeroLogo />
        </div>

        <Show when={showPlay()}>
          <Show
            when={running() === null}
            fallback={
              <button type="button" class="play-button button-danger" onClick={stop}>
                {t("home.stop")}
              </button>
            }
          >
            <button
              type="button"
              class="play-button"
              disabled={starting() || list().length === 0}
              onClick={play}
            >
              ✦ {starting() ? t("home.starting") : t("home.play")} ✦
            </button>
          </Show>

          <div class="hero-actions">
            <A href="/instances" class="hero-action">
              {t("home.manageInstances")}
            </A>
            <A href={currentId() ? `/mods?instance=${currentId()}` : "/mods"} class="hero-action">
              {t("home.manageMods")}
            </A>
          </div>

          <Show
            when={list().length > 0}
            fallback={
              // An empty picker reads as "broken"; with nothing to pick,
              // offer the one action that leads somewhere.
              <A href="/instances" class="hero-create">
                {t("home.createFirst")}
              </A>
            }
          >
            <div class="version-row">
              <label for="home-instance">{t("home.selectInstance")}</label>
              <select
                id="home-instance"
                class="instance-picker"
                value={currentId()}
                disabled={starting() || running() !== null}
                onChange={(e) => setSelected(e.currentTarget.value)}
              >
                <For each={list()}>
                  {(instance) => (
                    <option value={instance.id} selected={instance.id === currentId()}>
                      {instance.name} · {instance.minecraftVersion}
                    </option>
                  )}
                </For>
              </select>
            </div>
          </Show>
        </Show>

        <Show when={busy()}>
          <div class="hero-status">
            <Show when={installStatus()}>
              {(status) => (
                <div class="card install-card">
                  <div class="install-header">
                    <span>{status().phase}</span>
                    <Show when={status().filesTotal > 0}>
                      <span class="muted">
                        {status().filesDone}/{status().filesTotal} ·{" "}
                        {formatBytes(status().bytesDone)}
                        <Show when={status().bytesPerSec > 0}>
                          {" "}
                          · {formatBytes(status().bytesPerSec)}/s
                        </Show>
                      </span>
                    </Show>
                  </div>
                  <div class="progress-track">
                    <div
                      class="progress-fill"
                      style={{
                        width: `${((installFraction(status()) ?? 0) * 100).toFixed(1)}%`,
                      }}
                    />
                  </div>
                  <button type="button" onClick={() => backend.cancelPlay()}>
                    {t("home.cancel")}
                  </button>
                </div>
              )}
            </Show>

            <Show when={lastExit()}>
              {(exit) => (
                <div class={`card exit-card exit-${exit().class}`}>
                  <strong>{t(`exit.${exit().class}`)}</strong>
                  <Show when={exit().detail !== t(`exit.${exit().class}`)}>
                    <p class="muted">{exit().detail}</p>
                  </Show>
                </div>
              )}
            </Show>

            <Show when={running() !== null || lastExit() !== null}>
              <Console />
            </Show>
          </div>
        </Show>
      </div>

      <Show when={showNews()}>
        <NewsPanel />
      </Show>

      <Show when={!busy()}>
        <div class="home-cards">
          <For each={cards()}>
            {(id) => (
              <HomeCard id={id} instanceId={currentId()} instance={current()} />
            )}
          </For>
        </div>
      </Show>
    </section>
  );
}

/** The wordmark. A theme replaces it with `assets/logo.*`; reactive, so the
 *  swap happens whenever the theme lands or reloads. */
function HeroLogo() {
  return (
    <Show
      when={themeAsset("logo")}
      fallback={<h1 class="home-title">{t("app.name")}</h1>}
    >
      {(url) => (
        <div
          class="hero-logo-image"
          style={{ "background-image": url() }}
          role="img"
          aria-label={t("app.name")}
        />
      )}
    </Show>
  );
}

/** Release notes for the build actually running, parsed from CHANGELOG.md. */
function NewsPanel() {
  const [entries] = createResource(() => backend.changelog());

  return (
    <aside class="home-news card">
      <div class="card-header">
        <h2>{t("home.whatsNew")}</h2>
      </div>
      <div class="news-list">
        <For each={entries()}>
          {(entry, index) => (
            <div class="news-item">
              <div class="news-icon" aria-hidden="true">
                ✦
              </div>
              <div class="news-body">
                <div class="news-title">
                  <span>{entry.title || entry.version}</span>
                  <Show when={index() === 0}>
                    <span class="badge news-badge">{t("home.newBadge")}</span>
                  </Show>
                </div>
                <For each={entry.highlights}>
                  {(line) => <p class="muted news-line">{line}</p>}
                </For>
                <span class="muted news-date">
                  v{entry.version}
                  <Show when={entry.date}> · {entry.date}</Show>
                </span>
              </div>
            </div>
          )}
        </For>
      </div>
      <A href="/settings" class="card-footer-link">
        {t("home.viewAllUpdates")}
      </A>
    </aside>
  );
}

function HomeCard(props: {
  id: string;
  instanceId: string;
  instance: Instance | null;
}) {
  return (
    <>
      <Show when={props.id === "mods"}>
        <ModsCard instanceId={props.instanceId} />
      </Show>
      <Show when={props.id === "instance"}>
        <InstanceCard instance={props.instance} />
      </Show>
      <Show when={props.id === "system"}>
        <SystemCard />
      </Show>
      <Show when={props.id === "versions"}>
        <VersionsCard />
      </Show>
      <Show when={props.id === "cosmetics"}>
        <UnbuiltCard titleKey="home.card.cosmetics" bodyKey="home.card.cosmeticsBody" />
      </Show>
      <Show when={props.id === "friends"}>
        <UnbuiltCard titleKey="home.card.friends" bodyKey="home.card.friendsBody" />
      </Show>
    </>
  );
}

/**
 * A card whose feature does not exist yet. It keeps the dashboard's shape
 * without pretending to hold data — the alternative would be inventing
 * cosmetics and friends the launcher cannot actually provide.
 */
function UnbuiltCard(props: { titleKey: string; bodyKey: string }) {
  return (
    <div class="card home-card home-card-empty">
      <div class="card-header">
        <h2>{t(props.titleKey)}</h2>
      </div>
      <p class="muted">{t(props.bodyKey)}</p>
    </div>
  );
}

function ModsCard(props: { instanceId: string }) {
  const [view] = createResource(
    () => props.instanceId,
    (id) => (id ? backend.listMods(id) : Promise.resolve(null)),
  );
  const enabled = () => view()?.mods.filter((m) => m.enabled) ?? [];

  return (
    <div class="card home-card">
      <div class="card-header">
        <h2>{t("home.card.mods")}</h2>
        <span class="badge">{enabled().length}</span>
      </div>
      <Show
        when={enabled().length > 0}
        fallback={<p class="muted">{t("home.card.noMods")}</p>}
      >
        <div class="mini-list">
          <For each={enabled().slice(0, 4)}>
            {(mod) => (
              <div class="mini-row">
                <span class="mini-name">{mod.name}</span>
                <span class="muted">{mod.version}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
      <A href={props.instanceId ? `/mods?instance=${props.instanceId}` : "/mods"} class="card-footer-link">
        {t("home.card.manageMods")}
      </A>
    </div>
  );
}

function InstanceCard(props: { instance: Instance | null }) {
  const instance = () => props.instance;

  return (
    <div class="card home-card">
      <div class="card-header">
        <h2>{t("home.card.instance")}</h2>
      </div>
      <Show when={instance()} fallback={<p class="muted">{t("home.noInstances")}</p>}>
        {(i) => (
          <div class="mini-list">
            <div class="mini-row">
              <span class="muted">{t("home.card.name")}</span>
              <span class="mini-name">{i().name}</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.version")}</span>
              <span>{i().minecraftVersion}</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.loader")}</span>
              <span>{i().loader?.kind ?? t("home.card.vanilla")}</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.memory")}</span>
              <span>
                {i().java.maxRamMb ? `${i().java.maxRamMb} MB` : t("home.card.auto")}
              </span>
            </div>
          </div>
        )}
      </Show>
      <A href="/instances" class="card-footer-link">
        {t("home.card.manageInstances")}
      </A>
    </div>
  );
}

function SystemCard() {
  const [hardware] = createResource(() => backend.detectHardware());
  const [java] = createResource(() => backend.detectJava());

  return (
    <div class="card home-card">
      <div class="card-header">
        <h2>{t("home.card.system")}</h2>
        <Show when={java()}>{(list) => <span class="badge">{list().length}</span>}</Show>
      </div>
      <Show when={hardware()} fallback={<p class="muted">{t("home.card.detecting")}</p>}>
        {(hw) => (
          <div class="mini-list">
            <div class="mini-row">
              <span class="muted">{t("home.card.cpu")}</span>
              <span>
                {hw().cpuThreads} {t("home.card.threads")}
              </span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.ram")}</span>
              <span>{Math.round(hw().totalRamMb / 1024)} GB</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.suggested")}</span>
              <span>{hw().recommendedHeapMb} MB</span>
            </div>
            <Show when={(java()?.length ?? 0) > 0}>
              <div class="mini-row">
                <span class="muted">{t("home.card.java")}</span>
                <span>{java()!.map((j) => j.major).join(", ")}</span>
              </div>
            </Show>
          </div>
        )}
      </Show>
      <A href="/performance" class="card-footer-link">
        {t("home.card.openPerformance")}
      </A>
    </div>
  );
}

function VersionsCard() {
  const [manifest] = createResource(() => backend.listMinecraftVersions(false));

  return (
    <div class="card home-card">
      <div class="card-header">
        <h2>{t("home.card.versions")}</h2>
      </div>
      <Show when={manifest()} fallback={<p class="muted">{t("home.card.detecting")}</p>}>
        {(m) => (
          <div class="mini-list">
            <div class="mini-row">
              <span class="muted">{t("home.card.latestRelease")}</span>
              <span class="mini-name">{m().manifest.latest.release}</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.latestSnapshot")}</span>
              <span>{m().manifest.latest.snapshot}</span>
            </div>
            <div class="mini-row">
              <span class="muted">{t("home.card.known")}</span>
              <span>{m().manifest.versions.length}</span>
            </div>
            <Show when={m().source === "cacheStale"}>
              <p class="muted">{t("versions.stale")}</p>
            </Show>
          </div>
        )}
      </Show>
      <A href="/versions" class="card-footer-link">
        {t("home.card.browseVersions")}
      </A>
    </div>
  );
}
