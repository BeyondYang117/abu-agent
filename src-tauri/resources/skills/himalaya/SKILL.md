---
id: himalaya
name: himalaya
description: Read, search, compose, reply, and organize email via the Himalaya CLI (IMAP/SMTP). Activate when the user asks about mail, inbox, sending email, or when an email connector is configured.
recommended-tools:
  - bash
---

# Himalaya Email CLI

Use the `himalaya` binary for IMAP/SMTP mail from the shell via `bash` (run_command).

## Prerequisites

1. `himalaya` available — install manually via the Kivio Email connector, or install to system PATH yourself (`himalaya --version`)
2. Kivio email connector saved accounts → `~/.config/himalaya/config.toml`
3. Use `--account <id>` when multiple accounts exist (account id is shown in runtime context)

## Read / search

```bash
himalaya folder list
himalaya envelope list
himalaya message read <id>
himalaya envelope list from alice@example.com subject invoice
himalaya envelope list --output json
```

## Write

```bash
himalaya message write
himalaya template write
himalaya template send < /tmp/message.txt
himalaya message reply <id>
himalaya message forward <id>
```

For attachments or rich bodies, read `references/message-composition.md` (MML syntax).

## Organize

```bash
himalaya message copy <id> <folder>
himalaya message move <id> <folder>
himalaya message delete <id>
himalaya flag add <id> --flag seen
```

## Safety

- Confirm with the user before sending, deleting, or bulk-moving mail.
- Do not echo passwords or paste secrets into chat.
- Quote exact message IDs when summarizing results.

## References

- `references/configuration.md` — account config, providers, folder aliases
- `references/message-composition.md` — MML compose syntax
