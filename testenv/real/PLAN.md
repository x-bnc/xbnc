# xBNC 1.0 test plan on real networks

Thank you for testing xBNC. This list is for checking version 1.0 on real IRC networks, with real people, before it is declared stable. You do not need to do all of it: each section can be tested on its own. Tick what works and, if something fails, send what [How to report](#how-to-report) asks for.

Priority networks: **Undernet, Libera.Chat, freenode, Rizon, EvilNet and IRCnet**. Any other network is useful too.

## 0. Preparation

1. **Install** on a Linux server or machine with systemd (requirements in `README.md`):

   ```bash
   unzip xbnc-1.0.0.zip && cd xbnc
   sudo ./scripts/install.sh               # new installation
   sudo ./scripts/update.sh                # over an existing installation
   ```

2. **More detailed log** while you test. In `/etc/xbnc/xbnc.toml`:

   ```toml
   [log]
   level = "info,xbncd=debug"
   ```

   Restart with `sudo systemctl restart xbnc` and go back to `info` when you finish. Passwords never appear in the log, but with `debug` nicks and channels do.
3. In the panel, **Your account → Time zone**: choose yours (for example `Europe/London`).
4. Add the networks from **Networks → Add network**. All of them are ready in the wizard:

   | Network | Server | Identification |
   | --- | --- | --- |
   | Undernet | irc.undernet.org:6667 (no TLS: Undernet does not offer it) | X, with Login on Connect |
   | Libera.Chat | irc.libera.chat:6697 (TLS) | SASL or certificate |
   | freenode | irc.freenode.net:6697 (TLS) | SASL |
   | Rizon | irc.rizon.net:6697 (TLS) | SASL or NickServ |
   | EvilNet | irc.evilnet.org:6697 (TLS) | AuthServ (X3) |
   | IRCnet | open.ircnet.net:6697 (TLS) | no NickServ; optional SASL with an account from sasl.ircnet.com |

   If a server does not accept TLS on 6697, change the network to 6667 without TLS and note it in your report.

## 1. Connection and IRCv3 (on every network)

- [ ] It connects and joins the saved channels.
- [ ] **Your account → Test report → Download report.** For each network, the file records:
  - the capabilities the server offers (`caps_offered`);
  - the ones xBNC turned on (`caps_acked`);
  - ISUPPORT and the last error.

  It is the most useful thing you can send.
- [ ] With a modern client (WeeChat, Goguma, gamja, The Lounge, Halloy) check:
  - joins with the account (extended-join);
  - away changes (away-notify);
  - host changes (chghost);
  - the user list with @+ at once (multi-prefix).
- [ ] With a classic client (mIRC, HexChat): none of that reaches it and everything looks normal.
- [ ] Change the short name of a network in the panel: the connection is **not** cut.
- [ ] Kill xBNC abruptly (`sudo systemctl kill -s KILL xbnc`) and connect the client again: at most a few seconds of history are repeated.

## 2. Undernet

- [ ] **Services login** module with the X service (or “Automatic”), your account and your password. When connecting, your host is `account.users.undernet.org` **before** joining any channel. Check it in your client's JOIN or with `/whois your_nick`.
- [ ] With Login on Connect turned off: xBNC sends `LOGIN` to X and `+x`, and the channels wait for the hidden host.
- [ ] With a wrong X password: a *status notice arrives and the channels are joined anyway after the wait.
- [ ] **Ignore** with SILENCE: `/msg *ignore add pest` and check `/silence` in your client.
- [ ] **Invite and unban**: in a channel of yours with +i (or with a ban on you), leave and join again. xBNC asks X for `INVITE` or `UNBAN`.
- [ ] **Character encoding**: in a channel with people using Latin-1 (old mIRC), accented letters look right. If someone does not see yours right, add the channel to “Send in that encoding to”.
- [ ] With an away nick longer than 12 characters, it is shortened (NICKLEN=12).

## 3. Libera.Chat and freenode

- [ ] SASL PLAIN with your account: “SASL authentication succeeded” in the log.
- [ ] Libera: **Client certificate** with SASL EXTERNAL (generate the certificate in the panel, `/msg NickServ CERT ADD`, reconnect).
- [ ] Nick taken when connecting, with **Services login** (NickServ) and “Get my nick back”: xBNC sends REGAIN (Libera) or GHOST (freenode).
- [ ] cap-notify: if the server adds or removes capabilities live, nothing odd happens.

## 4. Rizon

- [ ] NickServ through SASL or with **Services login**.
- [ ] GHOST with the nick taken.
- [ ] **Invite and unban** with ChanServ in a channel of yours with +i.

## 5. EvilNet

- [ ] **Services login** with AuthServ: `AUTH account password`.
- [ ] `+x` and hidden host (numeric 396) before joining the channels.
- [ ] Note whether the network offers SASL (`caps_offered` in the report).

## 6. IRCnet

- [ ] No NickServ: with **Services login** on “Automatic”, xBNC sends your password to nobody and says so in *status.
- [ ] `!` and `+` channels (CHANTYPES) and long nicks.
- [ ] **Get op back when left alone** in a channel of yours without +i or +k.

## 7. Modules (on the network you prefer)

- [ ] **Detached channels**:
  - with `/quote DETACH #xbnc` the channel disappears from the client;
  - a mention brings it back with the last messages;
  - `/join #xbnc` brings it back completely.
- [ ] **Hide flooding channels**: in a channel with a lot of traffic, or lowering the limit to 5 messages in 10 seconds.
- [ ] **Remove colours** in a channel with colours.
- [ ] **FiSH encryption** with someone who uses FiSH in mIRC, HexChat or WeeChat:
  - in a channel: `/msg *crypt setkey #xbnc key` on both sides;
  - in private: `/msg *crypt keyx nick`.
- [ ] **Scheduled tasks**: `mon-fri 08:00 /away Working` and `every 30m /msg #xbnc still here`, in your time zone.
- [ ] **Log files**: the times are in your time zone.
- [ ] **History search** with a piece of a word: `ployment` finds “deployment”.

## 8. Plugins (with real people in a test channel)

The 18 come installed: turn them on in the network from **Modules** (Plugins section). `/msg *name help` explains each one.

- [ ] Trivia: `!trivial`, answers, hint, `!trivial top` and your own questions with `/msg *trivia add`.
- [ ] Welcome: the first time, when people come back and personal greetings.
- [ ] Link titles: approve the sites in **Plugins → Sites it may connect to** and paste a YouTube, Wikipedia and GitHub link.
- [ ] Definitions (`!def`, `?word`), karma (`nick++`) and quotes (`!quote`).
- [ ] Weather: `!weather London` (approve open-meteo.com).
- [ ] News: one feed per channel (approve the site) and wait for the second check.
- [ ] Forbidden words (you need op): warning, kick and temporary ban.
- [ ] Games: `!8ball`, `!rps` and `!hangman`.
- [ ] The eight earlier ones: dice, polls, reminders, tell, autoresponder, channel stats, seen and anti-flood.

## 9. *status commands

- [ ] `/msg *status help`: the commands are grouped, with the name in bold and aligned in your client. Note it if in any client (mIRC, HexChat, phone) it does not look right.
- [ ] `/msg *status help addserver` and `/msg *status help channel`.
- [ ] Backup servers: `addserver` with another server of the network, `servers` and `jump server`.
- [ ] `disablechan`, `enablechan`, `movechan` and `channels`.
- [ ] `playbuffer #xbnc 20` and `setbuffer #xbnc 30`.
- [ ] With an administrator: `listusers`, `traffic`, `broadcast test` and `setmotd`, and connect another client to see the message of the day.
- [ ] `/msg *status language es` and `/msg *status language en`: the help changes language at once.

## 10. Clients and devices

Test at least:

- mIRC;
- HexChat;
- WeeChat or irssi;
- on the phone, a client with bouncer support (Goguma or Palaver).

With two devices at once, what you read on one is not repeated on the other.

## How to report

For each problem, send:

1. **The report** from Your account → Test report.
2. **The warnings and errors of the log** from that moment:
   `sudo journalctl -u xbnc --since "1 hour ago" | grep -E "WARN|ERROR"`.
   Review it before sending it: with `debug` it may contain nicks and channel names.
3. **What you did, what you expected and what happened**, with the network, the client and the approximate time.
4. **The version**: it is in the panel footer or with `/msg *status version`.

If the project is on GitHub, open an issue with the “Test report” template. If not, send it through the contact on the project page. Security flaws are not made public: follow `SECURITY.md`.
