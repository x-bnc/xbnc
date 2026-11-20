<a id="commands"></a>

# IRC commands

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

You type them in your client as `/msg *service command`. The answers come in your language.

### \*status

It is the main service: everything you can do with a network in the panel can also be done here. `/msg *status help` lists the commands **grouped and with their names in bold**, only those your account may use; `/msg *status help <command>` explains one with an example, and `/msg *status help <word>` searches (for example `help channel`). Names are not case-sensitive, and the names used by other bouncers are accepted too (`ListChans`, `ListMods`, `AddTrustedServerFingerprint`…), listed under each command as “Also accepted”.

```text
<*status> xBNC *status commands
<*status> Write them as /msg *status <command>. Details and an example: help <command>.
<*status> Channels
<*status>  channels Your channels: users, modes and state
<*status>  nicks <#channel>   Everyone in a channel, with their rank
<*status>  topics   The topic of every channel
```

Unless stated otherwise, any account may use a command. The others say who may: accounts **allowed to make changes** (every role except “limited”), **administrators** (administrator and superadministrator) or **superadministrators**. Administrative actions are recorded in the [audit log](USERS_AND_SECURITY.md#security).

#### General

| Command | What it does |
| --- | --- |
| `help [command]` | This list, or the details of one command. Without a command, lists every command you can use. With one, shows how it works and an example. You can also search: help chan.<br>Example: `/msg *status help addserver` |
| `version` | xBNC version.<br>Example: `/msg *status version` |
| `uptime` | How long xBNC and this network have been running.<br>Example: `/msg *status uptime` |
| `language <code>` | Your language, also for the web panel. Changes the language of xBNC's messages, the plugins and the web panel at once. Without a code it lists the installed languages.<br>Example: `/msg *status language en` |
| `seen` | When you were last connected.<br>Example: `/msg *status seen` |
| `showmotd` | This server's message of the day.<br>Example: `/msg *status showmotd` |
| `clients [user]` | Devices connected to this network. Administrators can name a user to see all of theirs.<br>Example: `/msg *status clients`<br>Also accepted: `listclients` |

#### Networks and connection

| Command | What it does |
| --- | --- |
| `status` | State of this network: server, TLS, nick, capabilities.<br>Example: `/msg *status status` |
| `networks` | Your networks and their state.<br>Example: `/msg *status networks`<br>Also accepted: `listnetworks` |
| `addnetwork <name> <server> [[+]port]` | Add a network to your account. A + before the port means TLS. Without a port: 6697 with TLS. Your nick is the one of this network. Then connect with user/name.<br>Example: `/msg *status addnetwork libera irc.libera.chat +6697` |
| `delnetwork <name> confirm` | Delete one of your networks and its history. Asks for confirmation: repeat the command adding "confirm". It cannot be undone.<br>Example: `/msg *status delnetwork test confirm` |
| `setnick <nick>` | Change the nick saved for this network.<br>Example: `/msg *status setnick y2k`<br>Only accounts allowed to make changes (not the limited role). |
| `connect` | Connect to the network again.<br>Example: `/msg *status connect`<br>Only accounts allowed to make changes (not the limited role). |
| `disconnect` | Disconnect and stay disconnected. xBNC stays disconnected from this network until you use connect.<br>Example: `/msg *status disconnect`<br>Only accounts allowed to make changes (not the limited role). |
| `jump [server]` | Reconnect, to the next server or to the one you name. Without a server, goes to the next one of the list (servers). With one, connects to it.<br>Example: `/msg *status jump irc2.undernet.org`<br>Only accounts allowed to make changes (not the limited role). |

#### Servers, certificates and addresses

| Command | What it does |
| --- | --- |
| `servers` | Servers of this network (main and backups). If a server fails, xBNC tries the next one in turn.<br>Example: `/msg *status servers`<br>Also accepted: `listservers` |
| `addserver <server> [[+]port] [password]` | Add a backup server. A + before the port means TLS. Without a port: the same as the main server. Up to 20.<br>Example: `/msg *status addserver irc2.undernet.org 6667`<br>Only accounts allowed to make changes (not the limited role). |
| `delserver <server> [port]` | Remove a server. If you remove the main server, the first backup becomes the main one.<br>Example: `/msg *status delserver irc2.undernet.org`<br>Only accounts allowed to make changes (not the limited role). |
| `addfingerprint <SHA-256>` | Trust a server certificate by its fingerprint. For servers with a self-signed or expired certificate. 64 hex digits, with or without colons. Applies on the next connection.<br>Example: `/msg *status addfingerprint 3F:9A:...:C2:1E`<br>Also accepted: `addtrustedserverfingerprint`<br>Only accounts allowed to make changes (not the limited role). |
| `delfingerprint <SHA-256>` | Stop trusting a certificate.<br>Example: `/msg *status delfingerprint 3F:9A:...:C2:1E`<br>Also accepted: `deltrustedserverfingerprint`<br>Only accounts allowed to make changes (not the limited role). |
| `fingerprints` | Trusted certificates of this network.<br>Example: `/msg *status fingerprints`<br>Also accepted: `listtrustedserverfingerprints` |
| `showbindhost` | Local address used to connect.<br>Example: `/msg *status showbindhost` |
| `setbindhost <IP>` | Local address for this network (vhost). One of this server's IP addresses. Applies on the next connection (jump).<br>Example: `/msg *status setbindhost 203.0.113.25`<br>Administrators only. |
| `clearbindhost` | Use the automatic address for this network.<br>Example: `/msg *status clearbindhost`<br>Administrators only. |
| `setuserbindhost <IP>` | Default local address for all your networks. A network's own address (setbindhost) takes priority.<br>Example: `/msg *status setuserbindhost 2001:db8::25`<br>Administrators only. |
| `clearuserbindhost` | Remove your default local address.<br>Example: `/msg *status clearuserbindhost`<br>Administrators only. |

#### Channels

| Command | What it does |
| --- | --- |
| `channels` | Your channels: users, modes and state.<br>Example: `/msg *status channels`<br>Also accepted: `listchans` |
| `nicks <#channel>` | Everyone in a channel, with their rank.<br>Example: `/msg *status nicks #xbnc`<br>Also accepted: `listnicks` |
| `topics` | The topic of every channel.<br>Example: `/msg *status topics` |
| `enablechan <#channels>` | Join saved channels that were disabled. Accepts several channels and the wildcards \* and ?.<br>Example: `/msg *status enablechan #xbnc #xbnc-dev`<br>Only accounts allowed to make changes (not the limited role). |
| `joinnow` | Join your channels now, without waiting for the hidden host. When you ask for a hidden host (/mode $nick +x in the commands on connect, or the Services login module), xBNC does not join any channel until the server hides your host, so your real address is never shown. This command joins them right away anyway.<br>Example: `/msg *status joinnow` |
| `disablechan <#channels>` | Leave channels but keep them saved. They are not joined again until enablechan. Accepts \* and ?.<br>Example: `/msg *status disablechan #xbnc*`<br>Only accounts allowed to make changes (not the limited role). |
| `attach <#channels>` | Show detached channels again.<br>Example: `/msg *status attach #xbnc` |
| `detach <#channels>` | Hide channels from your client (they keep recording). A mention brings the channel back. Accepts \* and ?.<br>Example: `/msg *status detach #xbnc`<br>Only accounts allowed to make changes (not the limited role). |
| `movechan <#channel> <position>` | Change a channel's place in the order. The order is the one in which xBNC joins and lists your channels.<br>Example: `/msg *status movechan #xbnc 1`<br>Only accounts allowed to make changes (not the limited role). |
| `swapchans <#channel1> <#channel2>` | Swap the places of two channels.<br>Example: `/msg *status swapchans #xbnc #xbnc-dev`<br>Only accounts allowed to make changes (not the limited role). |

#### History (buffers)

| Command | What it does |
| --- | --- |
| `playbuffer <#channel\|nick> [lines]` | Play back the history of a conversation. Sends you the last lines (50 by default, up to 1000).<br>Example: `/msg *status playbuffer #xbnc 100` |
| `clearbuffer <#channel\|nick>` | Delete the history of a conversation. Accepts \* and ?.<br>Example: `/msg *status clearbuffer bob`<br>Also accepted: `clearhistory` |
| `clearallbuffers` | Delete all the history of this network.<br>Example: `/msg *status clearallbuffers` |
| `clearallchannelbuffers` | Delete the history of every channel.<br>Example: `/msg *status clearallchannelbuffers` |
| `clearallquerybuffers` | Delete the history of every private conversation.<br>Example: `/msg *status clearallquerybuffers` |
| `setbuffer <#channel> <days\|forever\|none\|default>` | How long a channel's history is kept. forever: never deleted; none: not stored; default: the server setting; or a number of days.<br>Example: `/msg *status setbuffer #xbnc 90`<br>Only accounts allowed to make changes (not the limited role). |

#### Modules and plugins

| Command | What it does |
| --- | --- |
| `modules` | Modules and plugins of this network and their state.<br>Example: `/msg *status modules`<br>Also accepted: `listmods` |
| `availmods` | Every module and plugin, with what it does.<br>Example: `/msg *status availmods`<br>Also accepted: `listavailmods` |
| `enable <module\|plugin>` | Turn on a module or plugin on this network.<br>Example: `/msg *status enable away`<br>Also accepted: `loadmod`<br>Only accounts allowed to make changes (not the limited role). |
| `disable <module\|plugin>` | Turn it off.<br>Example: `/msg *status disable away`<br>Also accepted: `unloadmod`<br>Only accounts allowed to make changes (not the limited role). |
| `reloadmod <module\|plugin>` | Restart a module or plugin on this network.<br>Example: `/msg *status reloadmod trivia`<br>Also accepted: `updatemod`<br>Only accounts allowed to make changes (not the limited role). |
| `get <module\|plugin>` | Show its settings.<br>Example: `/msg *status get away` |
| `set <module> <setting> <value>` | Change a setting. For yes/no settings: on/off. See the names with get.<br>Example: `/msg *status set away delay 60`<br>Only accounts allowed to make changes (not the limited role). |
| `addline <module> <setting> <text>` | Add a line to a list setting.<br>Example: `/msg *status addline watch patterns *xbnc*`<br>Only accounts allowed to make changes (not the limited role). |
| `dellines <module> <setting>` | Empty a list setting.<br>Example: `/msg *status dellines watch patterns`<br>Only accounts allowed to make changes (not the limited role). |

#### Administration

| Command | What it does |
| --- | --- |
| `users` | Every account and whether it is connected.<br>Example: `/msg *status users`<br>Also accepted: `listusers`<br>Administrators only. |
| `allnetworks` | Every network of every user, with its state.<br>Example: `/msg *status allnetworks`<br>Also accepted: `listallusernetworks`<br>Administrators only. |
| `traffic` | Traffic and messages since xBNC started.<br>Example: `/msg *status traffic`<br>Administrators only. |
| `broadcast <message>` | Send a notice to every connected user.<br>Example: `/msg *status broadcast Restart at 23:00`<br>Administrators only. |
| `movenetwork <user> <network> <new user> [new name]` | Move a network to another user. The network reconnects as the new user, with its history.<br>Example: `/msg *status movenetwork ana libera bob`<br>Administrators only. |
| `rehash` | Re-read users, networks, channels and languages. For changes made with the xbncd command line. Changes in xbnc.toml need a restart.<br>Example: `/msg *status rehash`<br>Also accepted: `reload`<br>Administrators only. |
| `listports` | Ports xBNC listens on. Ports are set in xbnc.toml (or with the installer) and need a restart.<br>Example: `/msg *status listports`<br>Administrators only. |
| `saveconfig` | Save the settings. Not needed: xBNC saves every change at once.<br>Example: `/msg *status saveconfig`<br>Administrators only. |
| `setmotd <text>` | Set this server's message of the day. Users receive it from \*status when they connect. Up to 20 lines.<br>Example: `/msg *status setmotd Welcome to xBNC`<br>Superadministrators only. |
| `addmotd <text>` | Add a line to the message of the day.<br>Example: `/msg *status addmotd Rules: /msg *status help`<br>Superadministrators only. |
| `clearmotd` | Remove the message of the day.<br>Example: `/msg *status clearmotd`<br>Superadministrators only. |
| `restart [message]` | Restart xBNC. Every network reconnects. The message, if any, is sent to everyone first.<br>Example: `/msg *status restart Updating`<br>Superadministrators only. |
| `shutdown [message]` | Stop xBNC completely. It stays stopped until it is started again on the server (systemctl start xbnc).<br>Example: `/msg *status shutdown Maintenance`<br>Superadministrators only. |

#### Notes on these commands

- **Backup servers.** If connecting to a server fails, xBNC tries the next one on the list a second later, and after going through the whole list it waits as usual again (5 s, 10 s, 20 s… up to 5 min). If a server that was connected drops the connection, xBNC first goes back to that same server. Passwords of backup servers are stored encrypted. `jump` alone moves to the next one; `jump server` or `jump server:port`, to a particular one.
- **Trusted certificates.** For networks with a self-signed or expired certificate, or one with another name. xBNC still checks everything else about TLS and accepts only that particular certificate (its SHA-256 fingerprint). To see a server's fingerprint: `openssl s_client -connect irc.example.org:6697 </dev/null 2>/dev/null | openssl x509 -noout -fingerprint -sha256`
- **Local address (vhost).** Administrators only, because it uses the server's IP addresses. xBNC checks that the address belongs to the machine before saving it. The network's address takes priority over the user's. It applies on the next connection.
- **Disabled channels.** They stay saved with their key and settings, but xBNC does not join them when connecting. The panel marks them as “Disabled”. `enablechan` joins them straight away.
- **Channel order.** It is the order in which xBNC joins when connecting and in which `channels` and the panel list them. New channels go at the end.
- **Wildcards.** `enablechan`, `disablechan`, `attach`, `detach` and `clearbuffer` accept several names separated by spaces or commas, and the wildcards `*` and `?`: `disablechan #xbnc* #xbnc-dev`.
- **Message of the day.** A superadministrator sets it and every user receives it, as a notice from `*status`, each time a client connects. Up to 20 lines of 400 characters. Shown with `showmotd`.
- **Notices to everyone.** `broadcast` sends “\[Administrator notice\] text” to every running network; every connected client receives it. `restart` and `shutdown` with a message send it first.
- **Restarting and stopping.** `restart` restarts xBNC inside the same process (the networks reconnect). `shutdown` stops it completely and systemd does not start it again: it has to be started with `sudo systemctl start xbnc`.
- **Ports and xbnc.toml.** Ports and the rest of [xbnc.toml](CONFIGURATION.md#configuration) are not changed from IRC: `listports` shows them and `rehash` re-reads users, networks, channels and languages (the same as `sudo systemctl reload xbnc`). `saveconfig` exists out of habit: xBNC saves every change at once.

### \*notes

| Command | What it does |
| --- | --- |
| `list` | Your notes. |
| `get <key>` | Show a note. |
| `add <key> <text>` | Save a note (up to 400 characters). |
| `del <key>` | Delete a note. |

They are the same notes as in *Your account → Notes*.

### \*watch

Matches of the [Watch words](MODULES.md#mod-watch) module arrive here. `/msg *watch help` reminds you how to configure it.

### \*crypt

| Command | What it does |
| --- | --- |
| `setkey <#channel\|nick> [cbc:]<key>` | Save the FiSH key of a channel or private conversation (cbc: for CBC mode). Stored encrypted. |
| `delkey <#channel\|nick>` | Delete a key. |
| `keyx <nick>` | Agree on a key in private with DH1080 (the other person needs FiSH). |
| `list` | Conversations with a key. |

Type `+p` before a message to send it unencrypted. See [Message encryption](MODULES.md#mod-crypt).

### \*ignore

| Command | What it does |
| --- | --- |
| `add <nick or mask>` | Ignore someone (a nick, or nick!user@host with \*). |
| `del <nick or mask>` | Stop ignoring. |
| `list` | List of ignored people. |

### One service per plugin

Each active plugin answers on `*id` (`*trivia`, `*remind`, `*poll`…). With `help` or any text it does not understand, it explains how it is used. If a plugin has nothing to say, xBNC answers with its name, version, description and usage.

### Client commands

| Command | What it does |
| --- | --- |
| `/quote DETACH #channel` | Detaches a channel: it keeps recording, but does not appear in your client. |
| `/quote ATTACH #channel` or `/join #channel` | Shows a detached channel again, with the latest messages. |
| `/quote <alias> [arguments]` | Runs an [alias](MODULES.md#mod-alias) if your client does not send commands it does not know. |

> Always write something after the name: `/msg *poll help`. Some clients, such as HexChat, show their own /msg help (“Usage: MSG \<nick\> \<message\>”) if you write nothing after it.
