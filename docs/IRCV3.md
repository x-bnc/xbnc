<a id="ircv3"></a>

# IRCv3

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

xBNC negotiates with each server the capabilities it offers and xBNC wants, and offers each client its own. Each client only gets what it asked for: a classic client receives the usual messages (for example, the short JOIN and only the highest rank in the user list).

### With the servers

| Capability | What for |
| --- | --- |
| `server-time` | Real time of each message. |
| `message-tags` | Tags on messages. |
| `multi-prefix` | Every rank of a user (@+), not only the highest. |
| `extended-join` | Joins carry the account and the real name. |
| `away-notify` | Notices when someone goes away or comes back. |
| `account-notify` | Notices when someone identifies or stops being identified. |
| `account-tag` | Each message says the account of whoever sent it. |
| `chghost` | Changes of user or host without leaving and joining. |
| `userhost-in-names` | The user list with nick!user@host. |
| `invite-notify` | Invitations of other people to your channels. |
| `setname` | Real name changes. |
| `cap-notify` | The server adds or removes capabilities on the fly. |
| `sasl` | SASL identification (PLAIN and EXTERNAL towards the server; PLAIN from the clients). |

### With the clients

| Capability | What for |
| --- | --- |
| `server-time` | Real time of each message. |
| `message-tags` | Tags on messages. |
| `echo-message` | Your own messages come back confirmed by the server. |
| `batch` | Groups of messages (history, playback). |
| `sasl` | SASL identification (PLAIN and EXTERNAL towards the server; PLAIN from the clients). |
| `draft/chathistory` | The client asks for the history it needs (CHATHISTORY). |
| `draft/event-playback` | The history includes joins, parts, kicks and nick changes as real events. |
| `draft/read-marker` | Read markers shared between devices. |
| `soju.im/bouncer-networks` | All your networks in a single connection. |
| `soju.im/bouncer-networks-notify` | Notices of changes in your networks. |
| `extended-join` | Joins carry the account and the real name. |
| `away-notify` | Notices when someone goes away or comes back. |
| `account-notify` | Notices when someone identifies or stops being identified. |
| `account-tag` | Each message says the account of whoever sent it. |
| `chghost` | Changes of user or host without leaving and joining. |
| `multi-prefix` | Every rank of a user (@+), not only the highest. |
| `userhost-in-names` | The user list with nick!user@host. |
| `invite-notify` | Invitations of other people to your channels. |
| `setname` | Real name changes. |
| `cap-notify` | The server adds or removes capabilities on the fly. |

With `CAP LS 302`, `sasl=PLAIN` is announced. If the server does not support a capability a client asks for, xBNC still offers it when it can build it itself (for example, the extended JOIN with the data it knows). The test report records, per network, what the server offers (`caps_offered`), what xBNC turned on (`caps_acked`) and what each client asked for.
