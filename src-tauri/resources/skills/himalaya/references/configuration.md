# Himalaya Configuration (Kivio-managed)

When you connect email in **Kivio → Settings → Connectors**, Kivio writes `~/.config/himalaya/config.toml` with IMAP (read) and SMTP (send) settings.

## Multiple accounts

```bash
himalaya --account work envelope list
himalaya --account personal folder list
```

Account ids appear in the chat runtime context after saving settings.

## Gmail notes

- Use an [App Password](https://myaccount.google.com/apppasswords) if 2FA is enabled.
- Preset: `imap.gmail.com:993` (TLS), `smtp.gmail.com:587` (STARTTLS).

## Folder aliases (Gmail example)

If folder names differ from defaults, add aliases in config — Kivio presets use standard names; for Gmail you may need:

```toml
[accounts.gmail.folder.alias]
inbox = "INBOX"
sent = "[Gmail]/Sent Mail"
drafts = "[Gmail]/Drafts"
trash = "[Gmail]/Trash"
```

## Manual setup (without Kivio)

```bash
himalaya account configure
himalaya --version
```

See the [Himalaya book](https://pimalaya.org/himalaya/) for full options.
