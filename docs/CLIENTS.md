<a id="clients"></a>

# Connecting your IRC client

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

Any client that lets you set the server, port and server password works. No plugin is needed.

| What you want | Server password |
| --- | --- |
| A user with a single network | `alice:password` |
| Choose the network | `alice/example:password` |
| Choose the network and name the device | `alice/example@laptop:password` |
| The same login in another order | `alice@laptop/example:password` |

- **Server:** your domain or IP (`sudo xbncd info` shows it).
- **Port:** 6697 with SSL from anywhere; 6667 without SSL only from your local network.
- **Password:** the same one you use for the panel. You can also put `alice/example@laptop` in the user field (USER) and the password alone in PASS, or sign in with SASL PLAIN from the client.
- **One connection is one network:** for two networks, create two servers in your client (`alice/example` and `alice/other`). Clients with `soju.im/bouncer-networks` (Goguma, gamja, Halloy) see all your networks in a single connection.
- **Device:** the `@device` part is optional but recommended. Each device (`@laptop`, `@phone`, `@work`) gets its own pending history.
- If you installed by IP, the certificate is self-signed: turn on “accept invalid certificates” in your client.

You do not have to work any of this out: in the panel, *Networks → your network → Connect your IRC client* shows the real server, port and server password of your account for each network, ready to copy. Write the name of the device and your password there and it builds the complete line (the password is not sent or saved). `sudo xbncd info` shows the same on the server, with your real users and networks.

### From the internet or from your local network

With a domain, the server is not always the same:

| Where you are | Server | Port |
| --- | --- | --- |
| Anywhere (the internet) | Your domain, for example `bnc.example.org` | 6697 with SSL. The router must forward 6697 to the xBNC machine. |
| At home (the same network as xBNC) | The local IP of the xBNC machine, for example `192.168.1.50` | 6667 without SSL. 6697 also works, but the domain's certificate is not valid for the IP: turn on “accept invalid certificates”. |

From home, the domain only works if your router supports it (it is called NAT loopback or hairpin NAT); many do not, and the connection just fails. That is why the panel has the two options, *From the internet* and *From your local network*, with the right data for each.

### HexChat

```
Network List → Add: "xBNC Example"
Edit → Server: bnc.example.org/6697
Tick "Use SSL for all the servers on this network"
Login method: Server password (/PASS password)
Password: alice/example@hexchat:your-password
With a self-signed certificate: tick "Accept invalid SSL certificates"
```

### mIRC

```
The quickest way, in any window:
/server bnc.example.org +6697 alice/example@mirc:your-password

The + before the port turns on SSL. With a self-signed certificate, mIRC asks
the first time: click Accept. To save it: File → Select server → Add, with
the same details.
```

### WeeChat

```
/server add xbnc-example bnc.example.org/6697 -tls
/set irc.server.xbnc-example.password "alice/example@weechat:your-password"
/set irc.server.xbnc-example.capabilities "server-time,message-tags,batch,draft/chathistory,echo-message"
/connect xbnc-example

With a self-signed certificate: /set irc.server.xbnc-example.tls_verify off
```

### irssi

```
/network add -nick alice xbnc-example
/server add -tls -network xbnc-example -password alice/example@irssi:your-password bnc.example.org 6697
/connect xbnc-example

With a self-signed certificate, add -notls_verify to /server add
```

### Mobile

**Goguma** (Android) and **Palaver** (iOS) support bouncers: add the server `bnc.example.org`, port 6697 with TLS, user `alice` and your password. Goguma shows all your networks in a single connection and asks for the history with `CHATHISTORY`. To get notifications with the app closed, use [xBNC's notifications](PANEL.md#notifications) (push, Telegram…), or install the panel as an app.

### Others

KVIrc, Konversation, Textual, Halloy, The Lounge and Quassel (as a client of a normal IRC network) work with the same details: server, port with TLS and server password. Clients with `draft/chathistory` (Goguma, gamja, Kiwi IRC, Halloy) ask for the history themselves and get no automatic playback.

### If you get the port or SSL wrong

xBNC spots the usual mistakes and tells you in the client itself:

| What you see in the client | What is happening | What to do |
| --- | --- | --- |
| `Closing link: this port uses SSL/TLS. Enable SSL in your IRC client, or connect to port 6667 without SSL.` | You connect to 6697 without SSL. | Turn on SSL or use 6667 from home. |
| `Closing link: this port only accepts connections from the local network...` | You use 6667 from outside your network. | Use 6697 with SSL. |
| Certificate error or “invalid certificate” | Self-signed certificate (installation by IP). | Accept the certificate or move to a domain with Let's Encrypt. |
| `Wrong user or password (user "alice")…` | Wrong user or password. | Use the same user and password as in the panel: alice/example@device:password. |
| `"YOUR_PASSWORD" is the example…` | The example was copied without writing the password. | Write your password after the colon. This does not count as a failed attempt. |
| `There is no network called "…" in your account. Your networks: …` | The network in the server password does not exist (it is the short name of the network in the panel). | Use one of the names listed. |
| `Say which network to use in the server password…` | You have several networks and did not say which. | Add /network to the user. |
| `Too many failed logins from your address (…): it is blocked for N more minutes` | Your IP is blocked after failed attempts, on IRC and in the panel. A client that keeps reconnecting with a wrong password also counts. | Correct the password in the client (or close it) and wait, or ask an administrator to unblock it in Security → Blocked IPs. |

When the client connects with SSL to a port without SSL it cannot show any message: the service log says so (`client sent TLS to a plain-text port`).

### Networks without SASL (Undernet, QuakeNet)

Some networks do not use SASL: you log in with a services bot, X on Undernet or Q on QuakeNet. xBNC does it for you every time it connects. Turn on the [Services login](MODULES.md#mod-services) module (in the wizard, the *Account* step) with your account and password. If you prefer to do it by hand, write the commands in *Settings → Commands on connect*, one per line, as in your client:

```
/mode $nick +x
/msg x@channels.undernet.org login YOUR_ACCOUNT YOUR_PASSWORD
```

They are sent before joining channels, and they are stored encrypted. When the commands (or the module, with “Hide my IP”) ask for `+x`, xBNC does not join any channel, and holds the JOINs of your client too, until the network confirms the hidden host (numeric 396). Your real address is never shown in a channel. If the host is not hidden after the wait (for example, because the X login failed), the channels stay waiting and `*status` tells you why; they are joined as soon as the host is hidden, or right away with `/msg *status joinnow`.

### Client compatibility

The automatic tests use a standard IRC client, a client with every IRCv3 capability and a classic client without any. HexChat, mIRC, WeeChat and irssi have been used by hand with xBNC; the other clients in the list follow the same standards but have not been checked one by one. If something does not work with a particular client, report it with the client's name and version (see [Testing the beta](TESTING.md#beta)).
