<a id="troubleshooting"></a>

# Common problems

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

Always start with the log and with `*status`: `sudo journalctl -u xbnc -n 100 --no-pager` and, from your client, `/msg *status status`.

### Installation

| Symptom | Likely cause | Solution |
| --- | --- | --- |
| “The build failed” and “SIGKILL” or “Killed” in the output | The machine ran out of memory while building (it needs about 2 GB, swap included). | Add swap and run the installer again: sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile |
| “The ready-made binary does not run on this system” | The system is older than 2019 (glibc older than 2.28). | Nothing to do: the installer builds it by itself. It takes a few minutes. |
| “Rust could not be installed” | The server cannot reach https://sh.rustup.rs (firewall or proxy). | Allow HTTPS to sh.rustup.rs and static.rust-lang.org, or install Rust by hand and run the installer again. With the .zip of a release, on x86\_64, this step is not needed: the program comes ready-made. |
| “about 3 GB of free space are needed” | Not enough disk to build. | Free space, or put the xbnc folder on a disk with more room. |
| “systemd is required” | The system does not use systemd (Alpine, containers). | Use Docker (see Docker in Installation) or a distribution with systemd. |
| After git pull and update.sh, the version is still the old one | Without --build, update.sh takes the program already built in target/ by the previous installation. | Run sudo ./scripts/update.sh --build. |
| The service keeps restarting and the log says “cannot listen on 0.0.0.0:PORT” … “Address already in use” | Another program already uses that port. The installer checks the panel and IRC ports, but not 6667 without SSL, which is often taken by an IRC server on the same machine. | See which program uses it: sudo ss -ltnp \| grep PORT. Then change the port, for example sudo ./scripts/install.sh --reconfigure --plain-port 6668 (or --plain-port off), or stop the other program. |

### You cannot connect

| Symptom | Likely cause | Solution |
| --- | --- | --- |
| The client says “invalid certificate” | You installed by IP with a self-signed certificate. | Accept the certificate in the client or move to a domain: sudo ./scripts/install.sh --reconfigure --domain … --letsencrypt … |
| Wrong user or password (user “…”) | Wrong user or password. | The server password is user/network@device:password, with the panel user and password. The panel shows yours ready to copy in Networks → your network; sudo xbncd info lists them. |
| “YOUR\_PASSWORD” is the example | The example was copied without the password. | Write your password after the colon. It does not count as a failed attempt. |
| Too many failed logins from your address (…): it is blocked for N more minutes | IP blocked after failures, on IRC and in the panel. A client reconnecting by itself with a wrong password blocks it again and again. | Correct (or close) the client, then unblock the IP in Security → Blocked IPs, or wait. |
| There is no network called “…” in your account | The network in the server password is not one of yours. | Use the short name of the network as the panel shows it (the message lists them). |
| Say which network to use in the server password | You have several networks and did not say which. | Add /network to the user. |
| Your account has no networks yet | The account has no network, or it is disabled. | Add one in Networks → Add network, or enable it. |
| From home, the domain does not connect | Your router does not support NAT loopback (hairpin NAT). | From home use the local IP of the xBNC machine (From your local network, in the panel). |
| By the local IP, the client rejects the certificate | The certificate is for the domain (Let's Encrypt), not for the IP. | Use port 6667 without SSL from home, or turn on “accept invalid certificates”. |
| this port uses SSL/TLS. Enable SSL… | Your client connects without SSL to the port that uses SSL (8443 or 6697). | Turn on SSL or use 6667 from your local network. |
| this port only accepts connections from the local network | You use 6667 from outside. | Use the port with SSL (8443 or 6697). |
| The client does not connect and nothing appears in the log | The client does not reach the machine (IP, port or firewall). | sudo xbncd info shows the right port; from the client: openssl s\_client -connect IP:PORT (Linux or macOS) or Test-NetConnection IP -Port PORT (Windows). |
| Log: client sent TLS to a plain-text port | The client has SSL on against 6667. | Turn SSL off or use the port with SSL. |
| Log: TLS handshake failed | The client does not accept the self-signed certificate. | Turn on “accept invalid certificates” or use --domain with --letsencrypt. |

### Problems with a network

| Symptom | Likely cause | Solution |
| --- | --- | --- |
| \*status: connection failed: … invalid peer certificate | Invalid IRC server certificate (self-signed or expired) or missing root certificates. | sudo apt install ca-certificates; check the host and the TLS port. If the network uses its own certificate, trust it with /msg \*status addfingerprint FINGERPRINT. |
| \*status: registration timed out | Wrong port (TLS against a port without TLS or the other way round). | Check the network's port and TLS (Undernet: 6667 without TLS). |
| SASL authentication failed | Wrong SASL account or password. | Correct them in the network configuration. |
| SASL is configured but this server does not offer SASL PLAIN | The network does not support SASL. | Remove SASL and use Services login. |
| Your host is trying to (re)connect too fast -- throttled | The network slows down repeated reconnections from your IP. | Wait; xBNC waits at least 2 minutes. Do not click Connect several times. |
| The channels are not joined: “Your host is still not hidden after … seconds” | +x was asked for (commands on connect or Services login) but the network has not hidden the host, usually because the X login failed. | Check the account and password of X; xBNC joins as soon as the host is hidden. /msg \*status joinnow joins right away, with the real host. |
| The login with X, Q or AuthServ does not work | Wrong services account or password. | Check Services login; the reason arrives through \*status. |
| On IRCnet it does not identify | IRCnet has no NickServ; xBNC does not send the password on purpose. | Use SASL from sasl.ircnet.com on the network, or nothing. |
| The nick ends in “\_” | The nick was taken when connecting. | Turn on Keep nick or type /nick your-nick. |
| xBNC keeps changing my nick | Keep nick goes back to the saved nick. | /msg \*status setnick new, or turn the module off. |
| I see “�” instead of accented letters | Rare: xBNC converts Latin-1 to UTF-8. | If it is the other person who cannot see your accents, add their channel or nick in Character encoding. |
| The client certificate does not identify | The fingerprint is not in your account, the network does not use TLS or you did not reconnect. | Add the fingerprint (SHA-512 on Libera, SHA-1 on OFTC) and reconnect. |

### Modules and plugins

| Symptom | Likely cause | Solution |
| --- | --- | --- |
| /msg \*plugin only shows “Usage: MSG \<nick\> \<message\>” | That is your client's help: you wrote nothing after the name. | /msg \*poll help. |
| An alias does nothing | Your client does not send commands it does not know. | /quote name arguments. |
| The autoop module does not give op | You do not have op, the mask does not match or the other person does not answer the challenge. | Check the real user@host with /whois, and the key. |
| A plugin with web access does not answer | Its sites are not approved. | A superadministrator approves them in Plugins (“Sites it may connect to”). |
| A plugin has stopped on a network | It failed five times in a row. | Look at the Plugins log; turn it off and on again after fixing it. |
| No log files appear | The log module is off or there are no permissions on /var/lib/xbnc/logs. | Turn it on; the service log says if it cannot write. |
| Scheduled tasks run at another time | Your time zone is not set. | Your account → Time zone. |

### Panel and service

| Symptom | Likely cause | Solution |
| --- | --- | --- |
| The panel does not load (connection refused) | \[web\] is missing or it listens on 127.0.0.1. | sudo xbncd info; to redo the configuration, sudo ./scripts/install.sh --reconfigure. |
| “The page was out of date” | Old CSRF token after signing in again. | Reload the page. |
| “Request refused because it came from another site” | public\_url does not match the browser's address. | Correct public\_url or use that address. |
| After signing in you are back at the sign-in page | Secure cookie over HTTP without TLS. | Use HTTPS or localhost (or secure\_cookies = false, for testing only). |
| Everyone gets blocked behind Nginx | Without trust\_proxy, everything comes from 127.0.0.1. | trust\_proxy = true and the X-Forwarded-For header in Nginx. |
| secret key file … not found; run xbncd init first | Not initialised. | sudo -u xbnc xbncd init. |
| secret key … is accessible by other users | Key permissions too open. | sudo chmod 600 /var/lib/xbnc/secret.key && sudo chown xbnc:xbnc /var/lib/xbnc/secret.key. |
| listener … has no TLS certificate | Public port without TLS. | Add tls\_cert and tls\_key, use lan\_only or 127.0.0.1. |
| database schema version N is newer than this binary supports | You went back to an older binary. | Use the new binary or restore the previous backup. |
| Changes made from the command line do not show | The service was not reloaded. | sudo systemctl reload xbnc. |
| A new language does not appear in the panel | The file is not in locales\_dir, has another name or xBNC was not reloaded. | xbncd locales check FILE; copy it to /etc/xbnc/locales/CODE.json and sudo systemctl reload xbnc. The log says which languages were loaded. |
| The client disconnects on its own with a lot of traffic | The client's queue filled up. | Increase client\_queue; when it comes back it gets what was pending. |
| Playback was truncated | There were more than playback\_max pending messages. | Increase playback\_max or use a client with draft/chathistory. |
| You lost the setup code | — | sudo xbncd setup-code, or create the user from the command line. |
| You lost the phone with two-step verification | — | sudo -u xbnc xbncd user reset-2fa user. |
| You forgot the password of the only administrator | — | sudo -u xbnc xbncd user passwd user. |

### More detail in the log

```sh
# In /etc/xbnc/xbnc.toml:  [log] level = "info,xbncd=debug"
sudo systemctl restart xbnc
sudo journalctl -u xbnc -f
sudo journalctl -u xbnc --since "1 hour ago" -p warning
```

With `trace` every IRC line is logged (with credentials masked). Go back to `info` when done: with `debug`, nicks and channels appear.
