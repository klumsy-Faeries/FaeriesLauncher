import { createResource, createSignal, For, Show } from "solid-js";

import { backend } from "../ipc/backend";
import type { DeviceCodePrompt } from "../ipc/types";
import { t } from "../i18n";
import { pushToast } from "../toasts";

export function Accounts() {
  const [info, { refetch }] = createResource(() => backend.listAccounts());
  const [prompt, setPrompt] = createSignal<DeviceCodePrompt | null>(null);
  const [busy, setBusy] = createSignal(false);
  // Kept on the page until dismissed: a toast disappears before anyone can
  // read a message like "Minecraft services rejected the request (403)".
  const [failure, setFailure] = createSignal<string | null>(null);

  const signIn = async () => {
    setBusy(true);
    setFailure(null);
    try {
      const started = await backend.beginSignIn();
      setPrompt(started);
      // The backend holds the device code and polls Microsoft with it until
      // the user finishes in their browser.
      const account = await backend.completeSignIn();
      pushToast(t("accounts.signedIn", { name: account.name }), "info");
      setPrompt(null);
      await refetch();
    } catch (error) {
      setFailure(String(error));
      pushToast(String(error), "error");
      setPrompt(null);
    } finally {
      setBusy(false);
    }
  };

  const cancelSignIn = async () => {
    await backend.cancelSignIn();
    setPrompt(null);
    setBusy(false);
  };

  const act = async (action: () => Promise<unknown>) => {
    try {
      await action();
      await refetch();
    } catch (error) {
      pushToast(String(error), "error");
    }
  };

  return (
    <section class="page">
      <h1>{t("nav.accounts")}</h1>

      <Show when={info() && !info()!.signInAvailable}>
        <div class="card problem-card">
          <p>{t("accounts.needClientId")}</p>
        </div>
      </Show>

      <Show when={failure()}>
        {(reason) => (
          <div class="card problem-card">
            <p>{t("accounts.signInFailed", { reason: reason() })}</p>
            <button type="button" onClick={() => setFailure(null)}>
              {t("accounts.dismissError")}
            </button>
          </div>
        )}
      </Show>

      <Show when={prompt()}>
        {(p) => (
          <div class="card device-code">
            <h2>{t("accounts.codeTitle")}</h2>
            <p>{t("accounts.codeInstruction", { uri: p().verificationUri })}</p>
            <div class="user-code">{p().userCode}</div>
            <p class="muted">{t("accounts.codeWaiting")}</p>
            <button type="button" onClick={cancelSignIn}>
              {t("instances.cancel")}
            </button>
          </div>
        )}
      </Show>

      <div class="card create-row">
        <span class="muted">{t("accounts.signInHint")}</span>
        <button
          type="button"
          class="button-primary"
          disabled={busy() || !info()?.signInAvailable}
          onClick={signIn}
        >
          {t("accounts.signIn")}
        </button>
      </div>

      <Show
        when={(info()?.accounts.length ?? 0) > 0}
        fallback={<p class="muted">{t("accounts.empty")}</p>}
      >
        <For each={info()?.accounts}>
          {(account) => (
            <div class="card instance-row">
              <div class="instance-info">
                <span class="instance-name">{account.name}</span>
                <span class="muted">
                  {account.id}
                  {info()?.active === account.id ? ` · ${t("accounts.active")}` : ""}
                </span>
              </div>
              <div class="instance-actions">
                <Show when={info()?.active !== account.id}>
                  <button
                    type="button"
                    onClick={() => act(() => backend.setActiveAccount(account.id))}
                  >
                    {t("accounts.setActive")}
                  </button>
                </Show>
                <button
                  type="button"
                  onClick={() => act(() => backend.removeAccount(account.id))}
                >
                  {t("accounts.remove")}
                </button>
              </div>
            </div>
          )}
        </For>
      </Show>
    </section>
  );
}
