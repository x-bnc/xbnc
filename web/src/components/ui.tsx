import { useEffect, useId, useRef, useState, type ReactNode, type InputHTMLAttributes } from "react";
import { t } from "../lib/i18n";
import { Check, Copy, Info, AlertTriangle, XOctagon, X } from "lucide-react";
import { stateLabel, colorFor } from "../lib/format";

export function StatePill({ state }: { state: string }) {
  return (
    <span className={`pill pill-${state}`}>
      <span className="dot" aria-hidden />
      {stateLabel(state)}
    </span>
  );
}

export function Nick({ name }: { name: string }) {
  return (
    <span className="nick" style={{ color: colorFor(name) }}>
      {name}
    </span>
  );
}

export function Avatar({ name, size = 34, src }: { name: string; size?: number; src?: string | null }) {
  if (src) {
    return <img className="avatar avatar-img" src={src} alt="" width={size} height={size} style={{ width: size, height: size }} />;
  }
  return (
    <span className="avatar" style={{ background: colorFor(name), width: size, height: size }} aria-hidden>
      {name.slice(0, 1).toUpperCase()}
    </span>
  );
}

export function Spinner({ label = t("Loading") }: { label?: string }) {
  return (
    <span role="status">
      <span className="spinner" aria-hidden />
      <span className="sr-only">{label}</span>
    </span>
  );
}

export function Loading() {
  return (
    <div className="center">
      <Spinner />
    </div>
  );
}

export function Callout({ kind = "info", children }: { kind?: "info" | "warn" | "bad"; children: ReactNode }) {
  const Icon = kind === "warn" ? AlertTriangle : kind === "bad" ? XOctagon : Info;
  return (
    <div className={`callout callout-${kind}`} role={kind === "bad" ? "alert" : undefined}>
      <Icon size={18} aria-hidden />
      <div>{children}</div>
    </div>
  );
}

interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  mono?: boolean;
}

export function Field({ label, hint, error, mono, id, ...rest }: FieldProps) {
  const auto = useId();
  const fid = id ?? auto;
  return (
    <div className="field">
      <label htmlFor={fid}>{label}</label>
      <input
        id={fid}
        className={`input${mono ? " input-mono" : ""}`}
        aria-invalid={error ? true : undefined}
        aria-describedby={hint || error ? `${fid}-help` : undefined}
        {...rest}
      />
      {(error || hint) && (
        <span id={`${fid}-help`} className={error ? "error" : "hint"}>
          {error || hint}
        </span>
      )}
    </div>
  );
}

export function SelectField({
  label,
  hint,
  value,
  onChange,
  options,
  disabled,
}: {
  label: string;
  hint?: ReactNode;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <select id={id} className="input" value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      {hint && <span className="hint">{hint}</span>}
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  description,
  disabled,
  hideLabel,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
  /** Keep the label for screen readers only (the row shows it already). */
  hideLabel?: boolean;
}) {
  return (
    <label className={`switch${hideLabel ? " switch-bare" : ""}`}>
      <input type="checkbox" role="switch" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className={hideLabel ? "sr-only" : undefined}>
        <strong>{label}</strong>
        {description && <span className="small muted">{description}</span>}
      </span>
    </label>
  );
}

export function Modal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>("input, select, textarea, button:not(.icon-btn)");
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      prev?.focus();
    };
  }, [onClose]);
  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={ref} className={`modal${wide ? " modal-wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
        <div className="spread">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label={t("Close")}>
            <X size={20} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

/**
 * Confirmation for destructive actions. With `typeToConfirm`, the user must
 * type that word before the button is enabled.
 */
export function Confirm({
  title,
  children,
  confirmLabel,
  onConfirm,
  onClose,
  typeToConfirm,
  busy,
  primary,
}: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onClose: () => void;
  typeToConfirm?: string;
  busy?: boolean;
  /** A confirmation that is not destructive (blue button). */
  primary?: boolean;
}) {
  const [typed, setTyped] = useState("");
  const ok = !typeToConfirm || typed.trim() === typeToConfirm;
  return (
    <Modal title={title} onClose={onClose}>
      <div className="stack">{children}</div>
      {typeToConfirm && (
        <Field
          label={t("Type {typeToConfirm} to confirm", { typeToConfirm })}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          autoComplete="off"
          mono
        />
      )}
      <div className="row row-end">
        <button className="btn btn-ghost" onClick={onClose}>{t("Cancel")}{" "}</button>
        <button className={primary ? "btn btn-primary" : "btn btn-danger-solid"} disabled={!ok || busy} onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}

/** A value with a copy button. `display` is shown instead of `text` (to hide a password). */
export function CopyText({ text, label, display }: { text: string; label?: string; display?: string }) {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    setDone(true);
    setTimeout(() => setDone(false), 1500);
  };
  return (
    <div className="cmd">
      <code>{display ?? text}</code>
      <button className="icon-btn" onClick={copy} aria-label={label ?? t("Copy")} title={t("Copy")}>
        {done ? <Check size={16} /> : <Copy size={16} />}
      </button>
    </div>
  );
}

export function Sparkline({ values, color = "var(--accent)", label }: { values: number[]; color?: string; label: string }) {
  if (values.length < 2) {
    return <svg className="spark" role="img" aria-label={t("{label}: waiting for data", { label })} />;
  }
  const w = 200;
  const h = 34;
  const max = Math.max(...values, 1e-9);
  const pts = values.map((v, i) => [(i / (values.length - 1)) * w, h - 3 - (v / max) * (h - 6)]);
  const line = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`).join(" ");
  return (
    <svg className="spark" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label={label}>
      <path d={`${line} L${w} ${h} L0 ${h} Z`} fill={color} opacity="0.12" />
      <path d={line} fill="none" stroke={color} strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
    </svg>
  );
}

export function Tabs<T extends string>({
  value,
  onChange,
  items,
  label,
}: {
  value: T;
  onChange: (v: T) => void;
  items: { value: T; label: string; count?: number }[];
  label: string;
}) {
  return (
    <div className="tabs" role="tablist" aria-label={label}>
      {items.map((it) => (
        <button
          key={it.value}
          role="tab"
          className="tab"
          aria-selected={value === it.value}
          onClick={() => onChange(it.value)}
        >
          {it.label}
          {it.count !== undefined && <span className="faint"> {it.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function Empty({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <h2>{title}</h2>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}
