// Internal command system (§23).
//
// Every user-triggerable action is a command. Buttons, keyboard shortcuts,
// and the Ctrl+K palette all dispatch through here rather than implementing
// logic inline, so one action stays reachable from everywhere — including
// future scripting and plugins.
//
// Two kinds of command exist:
//
//  - **static**: registered once at startup (navigation, launcher actions);
//  - **provided**: produced on demand by a *provider* when the palette
//    opens, so live things (instances, settings, mod files) are searched
//    from current state rather than a stale snapshot.

export interface Command {
  id: string;
  /** i18n key, or literal text when `titleText` is set instead. */
  titleKey?: string;
  /** Pre-resolved label, for dynamic commands naming user data. */
  titleText?: string;
  /** i18n key for the group heading in the palette. */
  categoryKey: string;
  /** Extra words this command should match on. */
  keywords?: string[];
  /** Right-aligned hint, e.g. a shortcut or a value. */
  hint?: string;
  run: () => void | Promise<void>;
}

export type CommandProvider = () => Command[] | Promise<Command[]>;

const commands = new Map<string, Command>();
const providers = new Set<CommandProvider>();

export function registerCommand(command: Command) {
  commands.set(command.id, command);
}

export function registerCommands(list: Command[]) {
  for (const command of list) registerCommand(command);
}

/** Register a source of live commands, consulted each time search runs. */
export function registerProvider(provider: CommandProvider): () => void {
  providers.add(provider);
  return () => providers.delete(provider);
}

export function runCommand(id: string): boolean {
  const command = commands.get(id);
  if (!command) return false;
  void command.run();
  return true;
}

export function staticCommands(): Command[] {
  return [...commands.values()];
}

/** Static commands plus everything the providers currently offer. */
export async function allCommands(): Promise<Command[]> {
  const dynamic = await Promise.all(
    [...providers].map(async (provider) => {
      try {
        return await provider();
      } catch {
        // A failing provider must not take the whole palette down.
        return [];
      }
    }),
  );
  return [...commands.values(), ...dynamic.flat()];
}

/**
 * Subsequence match with a light relevance score: earlier and more
 * contiguous matches rank higher, and a prefix match ranks highest. Returns
 * `null` when the query does not match at all.
 */
export function score(query: string, text: string): number | null {
  if (!query) return 0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.startsWith(q)) return 1000 - t.length;
  const direct = t.indexOf(q);
  if (direct >= 0) return 500 - direct - t.length / 100;

  // Fall back to a fuzzy subsequence walk.
  let ti = 0;
  let points = 0;
  let streak = 0;
  for (const char of q) {
    const found = t.indexOf(char, ti);
    if (found < 0) return null;
    streak = found === ti ? streak + 1 : 0;
    points += 10 + streak * 5 - Math.min(found - ti, 10);
    ti = found + 1;
  }
  return points;
}

/** Rank commands against a query. An empty query keeps registration order. */
export function search(
  query: string,
  list: Command[],
  resolve: (command: Command) => string,
): Command[] {
  if (!query.trim()) return list;
  const ranked: Array<{ command: Command; points: number }> = [];
  for (const command of list) {
    const haystacks = [resolve(command), ...(command.keywords ?? [])];
    let best: number | null = null;
    for (const hay of haystacks) {
      const points = score(query.trim(), hay);
      if (points !== null && (best === null || points > best)) best = points;
    }
    if (best !== null) ranked.push({ command, points: best });
  }
  ranked.sort((a, b) => b.points - a.points);
  return ranked.map((entry) => entry.command);
}
