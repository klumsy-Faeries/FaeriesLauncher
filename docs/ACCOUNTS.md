# Accounts and sign-in

## How sign-in works

The launcher uses Microsoft's **device-code** flow:

1. The launcher asks Microsoft for a short code.
2. You open Microsoft's own page and type that code.
3. Microsoft issues tokens to the launcher.
4. Those tokens are exchanged: Xbox Live → XSTS → Minecraft services →
   your profile. The profile request is also the ownership check (it answers
   404 for an account without Java Edition). The store entitlement list is
   deliberately not used: it is empty for Game Pass accounts.

The launcher never asks for, sees, or stores your Microsoft password. Tokens
are wrapped in a `Secret` type that redacts itself in every log and debug
output, and they are stored in the **Windows Credential Manager** (Keychain
on macOS, Secret Service on Linux) — never in a plaintext file.
`config/accounts.json` holds only your display name, UUID, XUID, and token
expiry.

## Setting up the client ID

Online sign-in needs an Azure application (client) ID that **Microsoft/Mojang
have approved** for the Minecraft services API. Since account-stealing
applications became a problem, only explicitly allow-listed client IDs may
call that API; unapproved ones get HTTP 403. This is Mojang's restriction,
not a limitation of this launcher.

### 1. Register the Azure application

In the [Azure portal](https://portal.azure.com) → **Microsoft Entra ID** →
**App registrations** → **New registration**:

- **Name**: anything (e.g. "Faeries Launcher").
- **Supported account types**: **Personal Microsoft accounts only**.
  This matters — the sign-in flow uses Microsoft's `consumers` tenant with
  the `XboxLive.signin` scope, which is required and only works with
  consumer Microsoft accounts. Work/school accounts cannot be used.
- **Redirect URI**: leave it empty. The device-code flow does not use one.

Then under **Authentication** → **Advanced settings**, set
**Allow public client flows** to **Yes**. Device code is a public-client
flow and Azure refuses it otherwise.

**Do not create a client secret.** This is a public client; the launcher
holds no secret and does not need one.

Copy the **Application (client) ID** and the **Directory (tenant) ID** from
the app's Overview page — the approval form asks for both.

### 2. Get it approved

Submit the application for Minecraft API access at
**<https://aka.ms/mce-reviewappid>**. The form asks you to accept the EULA
and provide your email, application name, client ID, and tenant ID.

After approval, allow up to 24 hours for it to take effect.

### 3. Enter it in the launcher

**Settings → Accounts → Microsoft client ID** — paste the Application
(client) ID and it saves immediately. The **Accounts** page's
"Sign in with Microsoft" button becomes enabled.

### Until then

Instances launch with an **offline session**: fine for singleplayer and LAN,
but online servers will reject it. The launcher does not and will not bypass
this.

## Multiple accounts

Sign in more than once to add accounts; the Accounts page switches the active
one. Removing an account deletes its stored tokens from the credential
manager as well as its metadata.

## If sign-in fails

The launcher reports the specific cause rather than a generic error:

| Message | Meaning |
|---|---|
| No Xbox Live profile | Sign in once at xbox.com to create one, then retry |
| Child account | The account must be added to a Microsoft family group |
| Does not own Minecraft: Java Edition | The account has no Java Edition entitlement |
| Sign-in was not completed in time | The device code expired — start again |
