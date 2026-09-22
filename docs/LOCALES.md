# Translating the launcher

Every user-facing string lives in a locale file. English (`en-US`) ships
inside the launcher; every other language is a plain JSON file you drop in,
with no rebuild and no code change.

## Add a language

1. Open your data directory (Settings → About), then create `locales/`.
2. Copy the launcher's [`locales/en-US.json`](../locales/en-US.json) into it
   and rename it to your language tag, e.g. `de-DE.json`.
3. Translate the values. Leave the keys alone.
4. Pick the language in **Settings → General → Language**.

## How partial translations behave

Every locale is merged *over* English, so you can translate a handful of
keys and ship it. Anything you have not translated shows the English text —
never a blank label or a raw key. That means a translation is useful from
its first line, and stays usable when the launcher adds new strings.

The launcher reports how much of a locale is translated, and warns about
keys that do not exist in English (almost always a typo), rather than
silently accepting them.

## Placeholders

Some strings contain `{name}`-style placeholders that are filled in at
runtime. Keep them exactly as they appear:

```json
"home.signedInAs": "Angemeldet als {name}",
"instances.deleted": "Instanz in den Papierkorb verschoben: {path}"
```

A missing placeholder is not an error, but the value it would have shown
disappears from the message.

## If something is wrong

A locale file that cannot be parsed is skipped with a warning and the
launcher shows English — a broken translation never blocks the UI.
