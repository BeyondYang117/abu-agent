// Pure helpers for the InputBar slash-command popover. Kept framework-free so
// the merge/match logic is unit-testable independently of React.
//
// Two kinds of slash commands coexist in the popover, distinguished by `kind`:
// - 'action' — built-in session actions (/help, /plan, /new ...) dispatched
//   locally (onNewChat etc.), never sent as a message.
// - 'skill'  — user skills surfaced as `/name`; selecting one only completes the
//   token (`/name `), and the whole string is sent on Enter. The backend parses
//   the slash trigger and pins the skill (see try_apply_skill_slash_trigger).

export type SlashCommandKind = 'action' | 'skill'

export interface SlashCommandDefinition {
  id: string
  slash: `/${string}`
  title: string
  description: string
  category: string
  keywords: string[]
  kind: SlashCommandKind
  argumentHint?: string
}

/** Minimal skill shape the popover needs. */
export interface SlashSkill {
  id: string
  name: string
  description?: string
  argumentHint?: string | null
  disableModelInvocation?: boolean
}

const SLASH_NAME_RE = /[^a-z0-9]+/g

/** Default `/slug(name)` trigger token for a skill (lowercased, dashed). */
export function skillSlashToken(skill: SlashSkill): `/${string}` {
  const slug = skill.name
    .toLowerCase()
    .replace(SLASH_NAME_RE, '-')
    .replace(/^-+|-+$/g, '')
  const token = slug || skill.id.toLowerCase()
  return `/${token}` as `/${string}`
}

/**
 * Merge built-in action commands with enabled skills into one popover list.
 * `disableModelInvocation` is NOT a filter here — it only gates model
 * auto-invocation, while an explicit user slash command may still trigger such
 * a skill. Callers pass already-enabled skills.
 */
export function buildSlashCommands(
  actions: SlashCommandDefinition[],
  enabledSkills: SlashSkill[],
): SlashCommandDefinition[] {
  const skillCommands = enabledSkills.map<SlashCommandDefinition>((skill) => {
    const slash = skillSlashToken(skill)
    return {
      id: `skill:${skill.id}`,
      slash,
      title: slash,
      description: skill.description?.trim() || skill.name,
      category: 'Skills',
      keywords: [skill.id, skill.name, slash.slice(1)].map((item) => item.toLowerCase()),
      kind: 'skill',
      argumentHint: skill.argumentHint?.trim() || undefined,
    }
  })
  return [...actions, ...skillCommands]
}

/** Case-insensitive fuzzy match against a slash command's searchable fields. */
export function commandMatches(command: SlashCommandDefinition, query: string): boolean {
  const normalized = query.trim().toLowerCase()
  if (!normalized) return true
  const searchable = [
    command.slash.slice(1),
    command.title,
    command.description,
    command.category,
    ...command.keywords,
  ].map((item) => item.toLowerCase())
  return searchable.some((item) => item.includes(normalized))
}
