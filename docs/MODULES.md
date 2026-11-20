<a id="modules"></a>

# Modules

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

Modules are native xBNC features, built into the program, that are turned on and configured **per network**: in the panel (*Modules*), with the API or from IRC with `/msg *status`. Changes apply at once, without reconnecting. Every setting is validated (types, ranges, lengths, no control characters). Passwords and keys are stored encrypted and the panel never shows them again: to change them, type them again.

```
/msg *status modules                         list and state
/msg *status enable keepnick                 turn on (disable to turn off)
/msg *status get away                        show settings
/msg *status set away delay 60               change a setting
/msg *status addline watch patterns *xbnc*   add a line to a list
/msg *status dellines watch patterns         empty a list
/msg *status availmods                       every module and plugin, with what it does
```

Setting types: **yes/no** (in `*status`, `true`, `on` or `1` for yes and `false`, `off` or `0` for no), **number** (with a minimum and maximum), **text**, **list** (one entry per line), **choice** (one of the values given) and **secret** (stored encrypted).

### Connection and account

<a id="mod-services"></a>

#### Services login

`services` · *new in 1.0*

Logs you in to X (Undernet), NickServ, Q or AuthServ when connecting and hides your IP before joining channels.

On Undernet it uses Login on Connect: the account goes in the server password and your real IP is never shown. If the network already has a server password, it logs in with /msg X login after connecting. Channels wait until the network confirms the login. With “Hide my IP”, no channel is ever joined before the network hides your IP, even if that takes longer than the wait (/msg \*status joinnow joins them anyway). The password is stored encrypted and never shown again.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Service (`service`) | choice | auto | Options: `auto` Automatic, by network, `x` X (Undernet), `nickserv` NickServ (Libera, Rizon, EvilNet…), `q` Q (QuakeNet), `authserv` AuthServ (GameSurge, AfterNet). |
| Account (`account`) | text | empty | Your account name on the service. With NickServ it can be empty if it is your nick. |
| Password (`password`) | secret | empty |   |
| Hide my IP (mode +x) (`hide_host`) | yes/no | yes | On X, Q and AuthServ. Other networks do it themselves. |
| Log in while connecting (Login on Connect) (`login_on_connect`) | yes/no | yes | Undernet only: the real IP is never shown. |
| Wait before joining channels (seconds) (`wait`) | number (0–120) | 15 | Until the network confirms the login. 0: do not wait. With “Hide my IP” the channels always wait for the hidden IP. |
| Get my nick back if it is taken (`recover_nick`) | yes/no | yes | With NickServ: asks for the nick (REGAIN or GHOST) after logging in. |

**Example**

```text
Connecting to irc.undernet.org:6667 with Login on Connect…
*** Your host is now y2k.users.undernet.org
* y2k has joined #xbnc
```

From IRC: `/msg *status enable services` and `/msg *status get services`

<a id="mod-invite"></a>

#### Invite and unban

`invite` · *new in 1.0*

If you cannot join a channel (+i, +k, +l or a ban), asks X or ChanServ to invite or unban you and joins again.

It tries at most three times every ten minutes per channel. You need access in the channel for the service to listen.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Channel service (`service`) | choice | auto | Options: `auto` Automatic, by network, `x` X (Undernet), `chanserv` ChanServ, `q` Q (QuakeNet). |
| Ask to remove my ban (`unban`) | yes/no | yes |   |
| Join when invited to one of my channels (`join_invites`) | yes/no | yes |   |

**Example**

```text
#xbnc: cannot join channel (+i)
<*status> Asking X to let you into #xbnc (+i).
* y2k has joined #xbnc
```

From IRC: `/msg *status enable invite` and `/msg *status get invite`

<a id="mod-cert"></a>

#### Client certificate (CertFP)

`cert`

Identifies you on the network with a certificate instead of a password. Enables SASL EXTERNAL.

Generate a certificate, add its fingerprint to your network account and reconnect. On Libera.Chat: connect identified and type /msg NickServ CERT ADD. Only works with TLS.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Use SASL EXTERNAL if the network offers it (`sasl_external`) | yes/no | yes |   |

**Example**

```text
<y2k> /msg NickServ CERT ADD
<NickServ> Added fingerprint 3f9a…c21e to your fingerprint list.
```

From IRC: `/msg *status enable cert` and `/msg *status get cert`

<a id="mod-keepnick"></a>

#### Keep nick

`keepnick` · *on by default*

If your nick is taken when connecting, xBNC takes it back as soon as it is free.

Checks every 30 seconds whether your nick is free and takes it back as soon as whoever holds it disconnects. If you change nick by hand from your client, it pauses until the next connection.

**Example**

```text
<*status> Your nick y2k is taken; connected as y2k_. I will take it back as soon as it is free.
* y2k_ is now known as y2k
```

From IRC: `/msg *status enable keepnick` and `/msg *status get keepnick`

### Channels

<a id="mod-stickychan"></a>

#### Sticky channels

`stickychan`

Channels you cannot leave by mistake: xBNC blocks /part and rejoins if you leave.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Channels (`channels`) | list | empty | One per line. |

**Example**

```
#xbnc
#xbnc-dev

If you type /part #xbnc, xBNC does not leave the channel.
```

From IRC: `/msg *status enable stickychan` and `/msg *status get stickychan`

<a id="mod-kickrejoin"></a>

#### Rejoin after a kick

`kickrejoin`

Rejoins the channel if you are kicked. Stops trying if you are kicked more than 3 times in a minute.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Wait before rejoining (seconds) (`delay`) | number (0–600) | 10 |   |

**Example**

```text
* bob has kicked you from #xbnc (test)
* y2k has joined #xbnc
```

From IRC: `/msg *status enable kickrejoin` and `/msg *status get kickrejoin`

<a id="mod-autoop"></a>

#### Automatic op and voice

`autoop`

Gives op, halfop or voice to the people you choose when they join your channels (if you have op).

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Wait before giving the mode (seconds) (`delay`) | number (0–30) | 3 |   |
| People (`entries`) | list | empty | One per line: mode (o, h or v), mask and, optionally, comma-separated channels (or \*) and a key. With a key, xBNC sends a challenge and only gives the mode if the other person answers with the same key from their own xBNC. |

**Example**

```
o *!*@home.example.org #xbnc
v friend!*@* *
o bob!*@* #xbnc shared-key
```

From IRC: `/msg *status enable autoop` and `/msg *status get autoop`

<a id="mod-autocycle"></a>

#### Get op back when left alone

`autocycle` · *new in 1.0*

If you are left alone in a channel without op, leaves and joins again to get it. Never in +i or keyed channels.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Only in these channels (`channels`) | list | empty | One per line. Empty: in all of them. |

**Example**

```text
* bob has left #xbnc
<*status> You are alone in #xbnc without op: leaving and joining again.
* y2k has joined #xbnc (with op)
```

From IRC: `/msg *status enable autocycle` and `/msg *status get autocycle`

<a id="mod-detach"></a>

#### Detached channels

`detach` · *new in 1.0*

Channels that keep recording everything but do not appear in your client. They come back if someone mentions you.

From your client: /quote DETACH #channel to detach and /join #channel to see it again. The latest messages are shown when it comes back.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Channels (`channels`) | list | empty | One per line. \* works (for example #\*-bots). |
| Show the channel if someone mentions my nick (`on_mention`) | yes/no | yes |   |
| Show the channel if one of these nicks writes (`nicks`) | list | empty | One per line. |

**Example**

```
/quote DETACH #xbnc     detaches the channel (it keeps recording)
/quote ATTACH #xbnc     or /join #xbnc: shows it again

If someone writes your nick in #xbnc, the channel comes back with the latest messages.
```

From IRC: `/msg *status enable detach` and `/msg *status get detach`

<a id="mod-flooddetach"></a>

#### Hide flooding channels

`flooddetach` · *new in 1.0*

If a channel suddenly fills with messages, it is hidden for a while so your client does not freeze. It keeps recording.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Messages (`messages`) | number (3–1000) | 25 | How many messages… |
| …in how many seconds (`seconds`) | number (1–600) | 10 |   |
| Quiet seconds before showing it again (`calm`) | number (10–3600) | 60 |   |

**Example**

```
Messages: 25
…in how many seconds: 10
Quiet seconds before showing it again: 60
```

From IRC: `/msg *status enable flooddetach` and `/msg *status get flooddetach`

### Messages

<a id="mod-away"></a>

#### Automatic away

`away` · *on by default*

Marks you away when you close all your clients and, when you come back, sums up what you missed.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Away message (`message`) | text | empty | Empty: “Away” in your language. |
| Wait before marking you away (seconds) (`delay`) | number (0–3600) | 60 |   |
| When I come back, tell me about private messages and mentions received (`summary`) | yes/no | yes |   |
| Nick while you are away (`nick`) | text | empty | Empty: unchanged. {nick} is your nick, for example {nick} |

**Example**

```
Away message: Back soon
Nick while away: {nick}|away

When you return, *status sums up: “While you were away: 3 private messages (bob, carol) and 2 mentions in #xbnc”.
```

From IRC: `/msg *status enable away` and `/msg *status get away`

<a id="mod-watch"></a>

#### Watch words

`watch` · *on by default*

Copies messages containing what you care about to the \*watch conversation, even if you were not connected.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Also tell me when someone mentions my nick (`mentions`) | yes/no | no |   |
| Patterns (`patterns`) | list | empty | One per line. \* matches any text: \*deploy\* finds “the deploy failed”. You can add a channel or nick after it to narrow it down. |

**Example**

```
*deploy*
*xbnc* #xbnc
*urgent* bob

Matches arrive in the *watch conversation.
```

From IRC: `/msg *status enable watch` and `/msg *status get watch`

<a id="mod-alias"></a>

#### Aliases

`alias` · *on by default*

Your own commands: you type /ghost bob and xBNC sends /msg NickServ GHOST bob.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Aliases (`aliases`) | list | empty | One per line: name and command. $1, $2… are the arguments, $1- all of them from the first, and $nick your nick. |

**Example**

```
ghost /msg NickServ GHOST $1
op /mode $1 +o $nick
greet /msg $1 hi, $1, how are you?

In your client: /ghost y2k   (or /quote ghost y2k if your client does not send it)
```

From IRC: `/msg *status enable alias` and `/msg *status get alias`

<a id="mod-ignore"></a>

#### Ignore

`ignore` · *new in 1.0*

Messages from people you ignore never reach any client or the history. On Undernet it uses SILENCE so they are not even sent.

On IRC: /msg \*ignore add nick, /msg \*ignore del nick and /msg \*ignore list.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Ignore these (`masks`) | list | empty | One per line: a nick or a nick!user@host mask (\* works). |
| Use SILENCE when the network allows it (`silence`) | yes/no | yes |   |

**Example**

```
/msg *ignore add pest
/msg *ignore add *!*@spam.example
/msg *ignore list
/msg *ignore del pest
```

From IRC: `/msg *status enable ignore` and `/msg *status get ignore`

<a id="mod-stripcontrols"></a>

#### Remove colours

`stripcontrols` · *new in 1.0*

Removes colours, bold, underline and blinking from messages, also in the history.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Only in these channels (`channels`) | list | empty | One per line. Empty: in all of them. |

**Example**

```
Only in these channels:
#xbnc

Before: \x0304,01red\x03 \x02bold\x02   After: red bold
```

From IRC: `/msg *status enable stripcontrols` and `/msg *status get stripcontrols`

<a id="mod-charset"></a>

#### Character encoding

`charset` · *new in 1.0*

Accents show correctly even when a channel uses Latin-1, and you can write in Latin-1 where needed.

xBNC always understands Latin-1 messages that are not UTF-8. This module picks the exact table and the channels or private chats where your messages are sent in that encoding (for people with very old clients).

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Legacy encoding (`legacy`) | choice | windows-1252 | Options: `windows-1252` Windows-1252 (the most common), `iso-8859-15` ISO-8859-15 (with €), `iso-8859-1` ISO-8859-1 (Latin-1). |
| Send in that encoding to (`send_legacy`) | list | empty | Channels or nicks, one per line. \* for all. Empty: always UTF-8. |

**Example**

```
Legacy encoding: Windows-1252
Send in that encoding to:
#xbnc
grandpa
```

From IRC: `/msg *status enable charset` and `/msg *status get charset`

<a id="mod-crypt"></a>

#### Message encryption (FiSH)

`crypt` · *new in 1.0*

Encrypts your messages in channels and private chats with shared keys, compatible with FiSH for mIRC, HexChat and irssi.

On IRC: /msg \*crypt setkey #channel key, /msg \*crypt keyx nick to agree a key in private and +p before a message to send it unencrypted. Keys are stored encrypted. FiSH protects from the network's servers; it is not a substitute for end-to-end encryption.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Keys (`keys`) | secret list | empty | One per line: channel or nick and key (cbc: first for CBC mode). They are stored encrypted: write all of them again to change them. |
| Mark for decrypted messages (`prefix`) | text | empty | For example \[encrypted\] . Empty: no mark. |
| Accept key exchanges (DH1080) (`key_exchange`) | yes/no | yes | When someone proposes a key in private. |

**Example**

```
/msg *crypt setkey #xbnc cbc:a-long-key
/msg *crypt keyx bob          agree on a key in private (DH1080)
/msg *crypt list
/msg *crypt delkey #xbnc
+p hello                      sends this message unencrypted
```

From IRC: `/msg *status enable crypt` and `/msg *status get crypt`

<a id="mod-schedule"></a>

#### Scheduled tasks

`schedule` · *new in 1.0*

Commands at a fixed time or every so often, in your time zone.

Pick the time zone in Your account. Tasks only run while the network is connected.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Tasks (`tasks`) | list | empty | One per line: time and command (08:00 /away Working), days and time (mon-fri 18:00 /msg #channel bye) or interval (every 30m /msg #channel still here). Days: mon tue wed thu fri sat sun, weekdays, weekend. |

**Example**

```
mon-fri 08:00 /away Working
mon-fri 15:00 /away
weekend 10:30 /msg #xbnc Good morning!
every 2h /msg #xbnc-dev still here
```

From IRC: `/msg *status enable schedule` and `/msg *status get schedule`

<a id="mod-ctcp"></a>

#### CTCP replies

`ctcp` · *on by default*

Always answers VERSION (by default with the xBNC version and its web site), answers PING and TIME when no client is connected and stops CTCP floods.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| CTCP requests allowed every 10 seconds (`flood_limit`) | number (1–100) | 5 |   |
| VERSION reply (`version`) | text | xBNC {version} Made by y2k - https://xbnc.org | {version} is replaced by the xBNC version. Empty: the default text. |
| CTCPs to ignore (`block`) | list | empty | One per line, for example TIME or FINGER. |

**Example**

```
CTCP requests allowed every 10 seconds: 5
Reply to VERSION: mIRC v7.77
CTCP to ignore:
FINGER
TIME
```

From IRC: `/msg *status enable ctcp` and `/msg *status get ctcp`

<a id="mod-block_motd"></a>

#### Hide the MOTD

`block_motd`

Does not show the server's message of the day to your clients.

**Example**

```
No settings. When on, your clients do not receive the server's message of the day.
```

From IRC: `/msg *status enable block_motd` and `/msg *status get block_motd`

### History and DCC

<a id="mod-events"></a>

#### Events in the history

`events` · *on by default*

Keeps joins, parts, kicks, nick changes, topics and modes next to the messages.

Modern clients get them as real events; the rest as messages from \*events when the history is played back.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Keep joins, parts and quits (`joins`) | yes/no | yes | In big channels they make a lot of noise. |
| Keep kicks, nick changes, topics and modes (`other`) | yes/no | yes |   |

**Example**

```
With “Store kicks, nick changes, topics and modes” on, when the history is played back
you also see: “bob changed the topic to: Meeting at 20:00”.
```

From IRC: `/msg *status enable events` and `/msg *status get events`

<a id="mod-log"></a>

#### Log files

`log`

Writes every conversation to daily text files, in the classic IRC log format.

The files stay on the server, in the logs folder next to the database (for example /var/lib/xbnc/logs/user/network/#channel/2026-11-08.log). Times are in your time zone (Your account).

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Delete files older than (days) (`retention_days`) | number (0–3650) | 0 | 0 keeps them forever. |
| Include events (joins, parts, kicks…) (`events`) | yes/no | yes |   |

**Example**

```
/var/lib/xbnc/logs/y2k/undernet/#xbnc/2026-11-11.log

[18:30:12] <bob> hello everyone
[18:30:20] * carol has joined
```

From IRC: `/msg *status enable log` and `/msg *status get log`

<a id="mod-route_replies"></a>

#### Replies to whoever asked

`route_replies` · *on by default*

Replies to WHOIS, WHO, LIST, NAMES or ban lists go only to the device that asked, not to all of them.

**Example**

```
If you run /whois bob from your phone, the answer reaches only the phone,
not the laptop that is also connected.
```

From IRC: `/msg *status enable route_replies` and `/msg *status get route_replies`

<a id="mod-dcc"></a>

#### DCC through xBNC

`dcc`

DCC file transfers and chats go through xBNC: the other person does not see your IP and they work even behind a router.

xBNC opens a port for each transfer and joins the two connections; nothing is stored on disk. On a server with a public IP it works with no setup; behind NAT, set the public IP in \[dcc\] public\_ip in xbnc.toml and open the \[dcc\] ports in the firewall.

| Setting | Type | Default | Detail |
| --- | --- | --- | --- |
| Largest transfer (MB) (`max_size_mb`) | number (0–1000000) | 4096 | 0 = no limit. |
| Seconds to accept a transfer (`wait`) | number (30–900) | 180 |   |
| Allow transfers from private addresses (`private_addresses`) | yes/no | no | When off, xBNC does not connect to internal networks (10.x, 192.168.x…) even if someone on IRC asks. |

**Example**

```
# xbnc.toml, only if the server is behind a router (NAT):
[dcc]
public_ip = "203.0.113.10"
ports = "50000-50100"
```

From IRC: `/msg *status enable dcc` and `/msg *status get dcc`

### Always on

Core features that need no turning on.

| Feature | Where it is configured |
| --- | --- |
| Commands on connect | In each network's configuration. |
| Remember channels | Always on: your /join and /part are remembered. |
| SASL and NickServ | In each network's configuration. |
| Brute-force blocking | In Settings and Security. |
| Notes | In Your account or with /msg \*notes. |
| Last seen | In Users and with /msg \*status seen. |
| Per-device history | Always on, with search on every network. |
| Web and IRC administration | This panel and /msg \*status. |
