import { useCallback, useEffect, useState, type FormEvent } from "react";
import { FileUp, KeyRound, Pencil, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { api, errorText, type Role, type User } from "../lib/api";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { ago, bytes, formatDate, roleHelp, roleLabel } from "../lib/format";
import { Avatar, Callout, Confirm, Field, Loading, Modal, SelectField, Switch } from "../components/ui";
import { locale, t } from "../lib/i18n";
import { ImportUsers } from "./Import";

const RANK: Record<Role, number> = { superadmin: 5, admin: 4, moderator: 3, user: 2, limited: 1 };
const ROLES: Role[] = ["superadmin", "admin", "moderator", "user", "limited"];

function canManage(me: Role, target: Role) {
  if (me === "superadmin") return true;
  if (me === "admin") return RANK[target] < RANK.admin;
  return false;
}
function canSuspend(me: Role, target: Role) {
  return canManage(me, target) || (me === "moderator" && RANK[target] <= RANK.user);
}

interface Usage {
  networks: number;
  max_networks: number;
  clients: number;
  max_clients: number;
  history_messages: number;
  history_bytes: number;
  max_history_mb: number | null;
}

function limitValue(v: string): number | null {
  return v.trim() === "" ? null : Math.max(0, Number(v));
}

export function UsersPage() {
  const me = useMe();
  const toast = useToast();
  const [users, setUsers] = useState<User[] | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<User | null>(null);
  const [resetting, setResetting] = useState<User | null>(null);
  const [deleting, setDeleting] = useState<User | null>(null);
  const [importing, setImporting] = useState(false);

  const load = useCallback(() => api.get<User[]>("/users").then(setUsers).catch((e) => toast.error(errorText(e))), []);
  useEffect(() => {
    load();
  }, [load]);

  const myRole = me.user.role;
  const assignable = ROLES.filter((r) => canManage(myRole, r));

  const toggle = async (u: User) => {
    try {
      await api.patch(`/users/${u.id}`, { enabled: !u.enabled });
      toast.ok(u.enabled ? t("{username} suspended. Their networks have been disconnected.", { username: u.username }) : t("{username} reactivated.", { username: u.username }));
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const remove = async () => {
    if (!deleting) return;
    try {
      await api.del(`/users/${deleting.id}`);
      toast.ok(t("User {username} deleted.", { username: deleting.username }));
      setDeleting(null);
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Users")}</h1>
          <p>{t("Each user has their own networks, history and password, and connects to IRC with their username.")}</p>
        </div>
        {me.permissions.manage_users && (
          <div className="row">
            {me.permissions.edit_settings && (
              <button className="btn" onClick={() => setImporting(true)}>
                <FileUp size={18} />{" "}{t("Import from another bouncer")}</button>
            )}
            <button className="btn btn-primary" onClick={() => setCreating(true)}>
              <Plus size={18} />{" "}{t("Create user")}</button>
          </div>
        )}
      </header>

      {!users ? (
        <Loading />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("User")}</th>
                <th>{t("Role")}</th>
                <th>{t("State")}</th>
                <th>{t("Networks")}</th>
                <th>{t("Connected")}</th>
                <th>{t("Last panel login")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {users.map((u) => {
                const self = u.id === me.user.id;
                return (
                  <tr key={u.id}>
                    <td>
                      <span className="row" style={{ gap: 10, fontWeight: 700 }}>
                        <Avatar name={u.username} size={30} src={u.avatar} />
                        {u.username}
                        {self && <span className="badge">{t("you")}</span>}
                        {u.totp_enabled && (
                          <span title={t("Two-step verification on")}>
                            <ShieldCheck size={16} color="var(--ok)" aria-label={t("2FA on")} />
                          </span>
                        )}
                      </span>
                    </td>
                    <td>
                      <span className={`badge${RANK[u.role] >= RANK.admin ? " badge-accent" : ""}`}>{roleLabel(u.role)}</span>
                    </td>
                    <td>{u.enabled ? <span className="badge badge-ok">{t("Active")}</span> : <span className="badge badge-bad">{t("Suspended")}</span>}</td>
                    <td>
                      {u.networks}
                      {u.max_networks != null && <span className="faint"> / {u.max_networks}</span>}
                    </td>
                    <td>{u.connected_clients ? (u.connected_clients === 1 ? t("1 device") : t("{count} devices", { count: u.connected_clients })) : <span className="faint">{t("No")}</span>}</td>
                    <td title={formatDate(u.last_login_at)}>{ago(u.last_login_at)}</td>
                    <td className="actions">
                      {!self && canSuspend(myRole, u.role) && me.permissions.suspend_users && (
                        <button className="btn btn-sm btn-ghost" onClick={() => toggle(u)}>
                          {u.enabled ? t("Suspend") : t("Re-enable")}
                        </button>
                      )}
                      {me.permissions.manage_users && canManage(myRole, u.role) && (
                        <>
                          <button className="icon-btn" title={t("Edit")} aria-label={t("Edit {username}", { username: u.username })} onClick={() => setEditing(u)}>
                            <Pencil size={17} />
                          </button>
                          <button className="icon-btn" title={t("Reset password")} aria-label={t("Reset {username}'s password", { username: u.username })} onClick={() => setResetting(u)}>
                            <KeyRound size={17} />
                          </button>
                          {!self && (
                            <button className="icon-btn" title={t("Delete")} aria-label={t("Delete {username}", { username: u.username })} onClick={() => setDeleting(u)}>
                              <Trash2 size={17} />
                            </button>
                          )}
                        </>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <section className="section">
        <h2>{t("What each role can do")}</h2>
        <div className="facts" style={{ gridTemplateColumns: "1fr" }}>
          {ROLES.map((r) => (
            <div className="fact" key={r}>
              <span style={{ fontWeight: 700 }}>{roleLabel(r)}</span>
              <span className="muted">{roleHelp(r)}</span>
            </div>
          ))}
        </div>
      </section>

      {creating && <CreateUser roles={assignable} onClose={() => setCreating(false)} onDone={() => { setCreating(false); load(); }} />}
      {editing && <EditUser user={editing} roles={assignable} self={editing.id === me.user.id} onClose={() => setEditing(null)} onDone={() => { setEditing(null); load(); }} />}
      {importing && <ImportUsers onClose={() => setImporting(false)} onDone={load} />}
      {resetting && <ResetPassword user={resetting} onClose={() => setResetting(null)} />}
      {deleting && (
        <Confirm title={t("Delete {username}", { username: deleting.username })} confirmLabel={t("Delete user")} typeToConfirm={deleting.username} onClose={() => setDeleting(null)} onConfirm={remove}>
          <p>{t("The account, its {n} networks and all its history are deleted. Its IRC clients are disconnected. If you only want to block access, suspend it.", { n: deleting.networks })}</p>
        </Confirm>
      )}
    </div>
  );
}

function CreateUser({ roles, onClose, onDone }: { roles: Role[]; onClose: () => void; onDone: () => void }) {
  const toast = useToast();
  const [f, setF] = useState({ username: "", password: "", role: "user" as Role, max_networks: "", max_clients: "" });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.post("/users", {
        username: f.username.trim(),
        password: f.password,
        role: f.role,
        max_networks: limitValue(f.max_networks),
        max_clients: limitValue(f.max_clients),
      });
      toast.ok(t("User {username} created.", { username: f.username }));
      onDone();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title={t("Create user")} onClose={onClose}>
      <form className="stack" onSubmit={submit}>
        {error && <Callout kind="bad">{error}</Callout>}
        <Field label={t("Username")} value={f.username} onChange={(e) => setF({ ...f, username: e.target.value })} required autoComplete="off" hint={t("They will use it to connect to IRC and the panel.")} />
        <Field label={t("Initial password")} type="password" value={f.password} onChange={(e) => setF({ ...f, password: e.target.value })} minLength={10} required autoComplete="new-password" hint={t("At least 10 characters. Ask them to change it when they sign in.")} />
        <SelectField label={t("Role")} value={f.role} onChange={(v) => setF({ ...f, role: v as Role })} options={roles.map((r) => ({ value: r, label: roleLabel(r) }))} hint={roleHelp(f.role)} />
        <div className="grid-2">
          <Field label={t("Maximum networks")} type="number" min={0} value={f.max_networks} onChange={(e) => setF({ ...f, max_networks: e.target.value })} placeholder={t("Global value")} />
          <Field label={t("Maximum devices")} type="number" min={0} value={f.max_clients} onChange={(e) => setF({ ...f, max_clients: e.target.value })} placeholder={t("Global value")} />
        </div>
        <div className="row row-end">
          <button type="button" className="btn btn-ghost" onClick={onClose}>{t("Cancel")}{" "}</button>
          <button className="btn btn-primary" disabled={busy}>{t("Create user")}</button>
        </div>
      </form>
    </Modal>
  );
}

function EditUser({ user, roles, self, onClose, onDone }: { user: User; roles: Role[]; self: boolean; onClose: () => void; onDone: () => void }) {
  const toast = useToast();
  const [role, setRole] = useState<Role>(user.role);
  const [enabled, setEnabled] = useState(user.enabled);
  const [maxN, setMaxN] = useState(user.max_networks?.toString() ?? "");
  const [maxC, setMaxC] = useState(user.max_clients?.toString() ?? "");
  const [maxH, setMaxH] = useState(user.max_history_mb?.toString() ?? "");
  const [usage, setUsage] = useState<Usage | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.get<Usage>(`/users/${user.id}/usage`).then(setUsage).catch(() => {});
  }, [user.id]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const body: Record<string, unknown> = { max_networks: limitValue(maxN), max_clients: limitValue(maxC), max_history_mb: limitValue(maxH) };
    if (!self && role !== user.role) body.role = role;
    if (!self && enabled !== user.enabled) body.enabled = enabled;
    try {
      await api.patch(`/users/${user.id}`, body);
      toast.ok(t("User updated."));
      onDone();
    } catch (err) {
      setError(errorText(err));
    }
  };
  return (
    <Modal title={t("Edit {username}", { username: user.username })} onClose={onClose}>
      <form className="stack" onSubmit={submit}>
        {error && <Callout kind="bad">{error}</Callout>}
        <SelectField
          label={t("Role")}
          value={role}
          onChange={(v) => setRole(v as Role)}
          options={(roles.includes(user.role) ? roles : [user.role, ...roles]).map((r) => ({ value: r, label: roleLabel(r) }))}
          hint={self ? t("You cannot change your own role.") : roleHelp(role)}
          disabled={self}
        />
        <Switch checked={enabled} onChange={setEnabled} disabled={self} label={t("Account enabled")} description={t("A suspended account cannot sign in and its networks are disconnected.")} />
        <div className="grid-2">
          <Field label={t("Maximum networks")} type="number" min={0} value={maxN} onChange={(e) => setMaxN(e.target.value)} placeholder={t("Global value")} hint={t("Empty: uses the value from Settings.")} />
          <Field label={t("Maximum devices")} type="number" min={0} value={maxC} onChange={(e) => setMaxC(e.target.value)} placeholder={t("Global value")} hint={t("IRC clients at the same time.")} />
        </div>
        <Field
          label={t("Maximum history space (MB)")}
          type="number"
          min={1}
          max={1000000}
          value={maxH}
          onChange={(e) => setMaxH(e.target.value)}
          placeholder={t("No limit")}
          hint={t("When it is exceeded, the oldest messages are deleted first. Checked every hour.")}
        />
        {usage && (
          <div className="facts">
            <div className="fact">
              <span>{t("Networks")}</span>
              <b>
                {usage.networks} / {usage.max_networks}
              </b>
            </div>
            <div className="fact">
              <span>{t("Connected devices")}</span>
              <b>
                {usage.clients} / {usage.max_clients}
              </b>
            </div>
            <div className="fact">
              <span>{t("History")}</span>
              <b>
                {bytes(usage.history_bytes)}
                {usage.max_history_mb != null && ` / ${usage.max_history_mb} MB`}
              </b>
              <span className="small faint">{t("{n} messages", { n: usage.history_messages.toLocaleString(locale()) })}</span>
            </div>
          </div>
        )}
        <div className="row row-end">
          <button type="button" className="btn btn-ghost" onClick={onClose}>{t("Cancel")}{" "}</button>
          <button className="btn btn-primary">{t("Save changes")}</button>
        </div>
      </form>
    </Modal>
  );
}

function ResetPassword({ user, onClose }: { user: User; onClose: () => void }) {
  const toast = useToast();
  const [pw, setPw] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.post(`/users/${user.id}/password`, { password: pw });
      toast.ok(t("{username}'s password changed. Their panel sessions have been closed.", { username: user.username }));
      onClose();
    } catch (err) {
      setError(errorText(err));
    }
  };
  return (
    <Modal title={t("New password for {username}", { username: user.username })} onClose={onClose}>
      <form className="stack" onSubmit={submit}>
        {error && <Callout kind="bad">{error}</Callout>}
        <Field label={t("New password")} type="password" value={pw} onChange={(e) => setPw(e.target.value)} minLength={10} required autoComplete="new-password" hint={t("It also changes the password of their IRC clients.")} />
        <div className="row row-end">
          <button type="button" className="btn btn-ghost" onClick={onClose}>{t("Cancel")}{" "}</button>
          <button className="btn btn-primary">{t("Change password")}</button>
        </div>
      </form>
    </Modal>
  );
}
