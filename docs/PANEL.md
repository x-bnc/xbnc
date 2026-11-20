# Web panel, history and notifications

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Web panel](#panel)
- [History and search](#history)
- [Notifications](#notifications)

<a id="panel"></a>

## Web panel

| Page | What it is for |
| --- | --- |
| Home | Live state of your networks (updated every 2 seconds) and, for administrators, memory, CPU, traffic, connections, blocked IPs, active alerts and new version notices. |
| Chat | Web IRC client: channels and private conversations of all your networks, with history. The panel counts as one of your devices. |
| Mentions | Everything said to you in private or where your nick was written, newest first. |
| Networks | Add (four-step wizard) and edit networks; control (connect, disconnect, reconnect); channels; connected devices (with a button to disconnect one); history with search and export; commands on connect; client certificate; the card “Connect your IRC client”. |
| Modules | Modules and plugins of each network. A network selector that works with any number of networks, and “Copy to other networks” (modules and plugins, except the certificate). |
| Plugins | Official and remote catalogue, one-click installation or uploading a .wasm, permissions and approved sites, turning them on or off on the server, uninstalling, live log and development mode (superadministrators). |
| Users | New accounts, roles, quotas of networks, devices and history, suspension, password reset, usage of each account and import from another bouncer. |
| Security | Sign-in attempts (IRC, panel and tokens), blocked IPs with a button to unblock them, open sessions and an audit log of every action. |
| Settings | History, limits, access protection, default language, alerts, backups, updates and restarting the service. |
| Your account | Profile photo (cropped to 256×256 in the browser; PNG, JPEG or WebP up to 256 KB; seen by you and the administrators), language, time zone, notifications, password, two-step verification, passkeys, linked external accounts, notes, API tokens, test report and open sessions. |

### Live settings

What you change in *Settings* is stored in the database, applies without restarting and takes priority over `xbnc.toml`. The ↺ icon next to a field goes back to the value of `xbnc.toml`.

| Group | Settings | Default |
| --- | --- | --- |
| History | Days of retention (0: forever); messages played back on reconnection; messages for a new device; CHATHISTORY limit. | 30 days; 1000; 100; 500 |
| Limits | Total connections; devices per user; networks per user; queue per client (lines before a slow client is disconnected). Each user can have exceptions in Users. | 1000; 10; 10; 4096 |
| Access protection | Failed attempts before blocking; window; length of the block; time to complete the login. | 5; 600 s; 900 s; 30 s |
| General | Default language for people who have not chosen one in Your account. It is saved and applied when clicked, also to whoever changes it. | the one in xbnc.toml |
| Alerts | See [Monitoring](ADMINISTRATION.md#monitoring). |   |
| Backups | Automatic yes/no, every how many hours (1–720) and how many to keep (1–365). | no; 24 h; 7 |

What is read at start (ports, certificates, database, secret key, the panel itself and the log level) is changed in `xbnc.toml` and needs `sudo systemctl restart xbnc`. The page says so. Superadministrators also have a button to restart the service.

### Accessibility and use on the phone

- Keyboard navigation with visible focus; dialogs close with Escape.
- Respects the system's “reduce motion”.
- On narrow screens the side bar opens with the menu button.
- The theme (light, or dark in black, yellow and grey) follows the system the first time; the moon or sun button changes it and remembers it in that browser.
- It can be installed as an app (PWA) to get push notifications.
- In English and Spanish, and in any language installed as a translation file (see [Translating xBNC](TRANSLATIONS.md#translations)). The language is changed on the sign-in page, in *Your account* or (the server default) in *Settings*, and applies at once to the panel and to xBNC's messages on IRC, also with the client connected. From IRC it is changed with `/msg *status language en`.

<a id="history"></a>

## History and search

xBNC stores the messages of channels and private conversations (and, with the [Events](MODULES.md#mod-events) module, joins, parts, kicks, nick changes, topics and modes). Each message carries the server's time if the network sends it.

### Searching

In *Networks → (network) → History*, or with the API. The search uses a full-text index:

- It finds whole words and word beginnings: `deplo` finds “deployment”.
- It ignores case and accents: `cafe` finds “Café”.
- Several words must all appear, in any order. In quotes, the exact phrase is searched: `"out of service"`.
- If nothing matches by word, it also looks for pieces in the middle of a word: `ployment` finds “deployment”.
- It combines with the conversation, sender and date filters.

### Export and delete

It exports to TXT or JSON (everything, or one conversation). A conversation or the whole history of the network is deleted from the panel, with `/msg *status clearbuffer` (or `clearallbuffers`, `clearallchannelbuffers`, `clearallquerybuffers`) or with the API.

### Retention and quotas

- **General:** *Settings → History*, days of retention (30 by default; 0, forever).
- **Per channel:** in the channels of each network, or with `/msg *status setbuffer`: the server setting, store nothing, forever or N days.
- **Per user:** an administrator can set a history space quota (`max_history_mb`) in *Users*; usage shows on each user's page.
- Space: about 150 bytes per stored message, including the search index.

### Log files

Besides the history, the [Log files](MODULES.md#mod-log) module writes daily text files in the classic IRC format to `/var/lib/xbnc/logs/user/network/#channel/YYYY-MM-DD.log`, with times in your time zone.

<a id="notifications"></a>

## Notifications

In *Your account → Notifications*, xBNC tells you about mentions and private messages.

| Option | What it does | Default |
| --- | --- | --- |
| Turn on notifications | Main switch. | no |
| Only when I have no client connected | No notice if you have an IRC client or the panel chat open. | yes |
| When someone writes my nick in a channel | Mentions. | yes |
| When I get a private message | Private messages. | yes |

| Channel | What you need |
| --- | --- |
| Browser and phone (push) | Click “Turn on in this device” from the panel open on that device. On the phone, install the panel as an app first. It can be turned on on several devices. |
| Telegram | Create a bot with @BotFather, write to it once and enter its token and your chat id (@userinfobot tells you). The panel never shows the full token again. |
| Discord | A channel webhook: Channel settings → Integrations → Webhooks. Notifications mention nobody. |
| E-mail | Your address. The administrator has to configure the \[smtp\] section of xbnc.toml. |
| Your own web address | An https URL that receives a POST with the notification in JSON. |

“Send a test notification” sends one to every configured channel. So as not to flood you, xBNC groups notifications: if many arrive in a row, those not sent are counted and the next notification says how many were skipped.

### Format of the notification at your own web address

```json
{
  "network": "undernet",
  "network_id": 1,
  "conversation": "#xbnc",
  "from": "bob",
  "text": "y2k, what time is the meeting tomorrow?",
  "private": false,
  "ts": 1762851240000,
  "skipped": 0
}
```

With push, tapping the notification opens the panel chat in that conversation.
