<a id="networks"></a>

# Networks

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

### Settings of a network

In the panel, *Networks → (your network) → Configuration*. Also with the API (`PATCH /networks/{id}`) or the command line.

| Setting | What it is |
| --- | --- |
| Short name | The one you use to log in (`alice/example`). Changing it does not cut the connection. |
| Server and port | Address of the network. Port 6697 with TLS by default. |
| TLS | Encrypted connection to the network. The server's certificate is always verified. |
| Nick | Your nick on the network. If it is taken when connecting, xBNC adds “\_” and, with [Keep nick](MODULES.md#mod-keepnick), goes back to it as soon as it is free. |
| User (ident) and real name | The USER fields. By default, the nick. |
| Server password | Sent with PASS when connecting. Stored encrypted and never shown again. |
| SASL account and password | SASL PLAIN identification. For SASL EXTERNAL, use a [client certificate](MODULES.md#mod-cert). |
| Enabled | When off, xBNC does not connect to that network. |
| Commands on connect | Lines sent after connecting and before joining the channels (see below). |
| Channels | Saved channels, with their key (encrypted). A /join or /part from the client is saved on its own. |
| Retention per channel | How long a channel's history is kept: the server setting, nothing, forever or N days. |
| Backup servers | Other servers of the same network that xBNC tries if the current one fails. With `/msg *status addserver` (see [IRC commands](COMMANDS.md#commands)). |
| Trusted certificates | SHA-256 fingerprints of server certificates accepted even if they do not validate (self-signed or expired). With `/msg *status addfingerprint`. |
| Local address (vhost) | The IP of this server used to connect to the network. With `/msg *status setbindhost` (administrators only). |
| Disabled channels and order | A disabled channel stays saved but is not joined; the order is the joining order when connecting. With `/msg *status disablechan` and `movechan`. |

### Commands on connect

They are written as in your client, one per line (up to 20, of 510 characters each), and stored encrypted because they often contain passwords. `$nick` is replaced by your current nick. Allowed: `/msg` (or `/privmsg`, `/query`), `/notice`, `/quote` (or `/raw`) and any IRC command with a slash (`/mode`, `/away`…). A line without a slash is sent as it is.

```
/mode $nick +x
/msg x@channels.undernet.org login YOUR_USER YOUR_PASSWORD
AWAY :Connected from xBNC
```

To identify with X, NickServ, Q or AuthServ the [Services login](MODULES.md#mod-services) module is better: it waits for the network to confirm the login before joining the channels, and on Undernet it uses Login on Connect.

### Undernet

Undernet uses the **X** service and has neither SASL nor TLS. Turn on [Services login](MODULES.md#mod-services) with your X account: with *Login on Connect* the account travels in the server password (`+x! account password`), so your real IP is never seen and you join the channels already as `account.users.undernet.org`.

```text
*** Your host is now y2k.users.undernet.org
* y2k has joined #xbnc
<*status> xBNC v1.0.0 - https://xbnc.org
```

- With [Invite and unban](MODULES.md#mod-invite), if you cannot join one of your channels, xBNC asks X for `INVITE` or `UNBAN`.
- [Ignore](MODULES.md#mod-ignore) uses `SILENCE`: messages from the people you ignore do not even reach xBNC.
- Nicks have at most 12 characters; the away nick is shortened automatically.
- Many Spanish-speaking channels use Latin-1: xBNC always understands it, and with [Character encoding](MODULES.md#mod-charset) you can write to them in Latin-1.
- The connection to Undernet is unencrypted because the network offers no TLS; the X password travels in clear text to the server, as with any client.

### Libera.Chat and freenode

Identification by **SASL**: put your account and password in the network configuration. On Libera you can also use a certificate ([Client certificate](MODULES.md#mod-cert)) with SASL EXTERNAL: generate the certificate in the panel, connect identified, type `/msg NickServ CERT ADD` and reconnect. If your nick is taken when connecting, [Services login](MODULES.md#mod-services) with “Recover my nick” sends `REGAIN` (Libera) or `GHOST` (freenode).

### Rizon

NickServ (Anope), by SASL or with Services login. ChanServ handles invitations and unbans.

### EvilNet

You identify with **NickServ** (`IDENTIFY account password`). Choose NickServ in Services login, or “Automatic”, which uses NickServ on EvilNet.

### QuakeNet

The **Q** service (`AUTH` to `Q@CServe.quakenet.org`) and `+x`. Services login does it for you; invite and unban use `INVITE` and `UNBANME`.

### IRCnet and EFnet

They have no NickServ: anyone could take that nick. That is why, in “Automatic”, xBNC **never sends your services password** on these networks and says so in `*status` and in the log. IRCnet offers optional SASL with accounts from sasl.ircnet.com. To keep op without services, use [Get op back when left alone](MODULES.md#mod-autocycle).

### OFTC and other networks with NickServ

Services login with NickServ, or commands on connect. For the client certificate, OFTC uses the SHA-1 fingerprint and Libera the SHA-512 one: the panel shows all three fingerprints (SHA-1, SHA-256 and SHA-512).
