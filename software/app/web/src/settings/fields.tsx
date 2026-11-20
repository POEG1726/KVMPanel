import { useEffect, useState, type ReactNode } from 'react';
import s from './settings.module.css';

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className={s.group}>
      {title && <h3>{title}</h3>}
      {children}
    </section>
  );
}

/** A label on the left, the control on the right. */
export function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className={s.row}>
      <span className={s.rowLabel}>
        {label}
        {hint && <small>{hint}</small>}
      </span>
      <span className={s.rowControl}>{children}</span>
    </div>
  );
}

interface TextProps {
  label: string;
  value: string;
  onCommit(v: string): void;
  type?: 'text' | 'number' | 'password';
  placeholder?: string;
  min?: number;
  max?: number;
  disabled?: boolean;
  list?: string;
  width?: number;
}

/** A text/number input that commits on blur or Enter, and only when the value changed. */
export function TextField({ label, value, onCommit, type = 'text', width, ...rest }: TextProps) {
  const [local, setLocal] = useState(value);
  useEffect(() => setLocal(value), [value]);
  const commit = () => {
    if (local !== value) onCommit(local);
  };
  return (
    <input
      className={s.input}
      style={width ? { width } : undefined}
      type={type}
      aria-label={label}
      value={local}
      onChange={(e) => setLocal(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === 'Enter' && commit()}
      {...rest}
    />
  );
}

export function Alert({ message }: { message: string | null }) {
  return message ? (
    <p role="alert" className={s.alert}>
      {message}
    </p>
  ) : null;
}

export function bytes(n: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v >= 10 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}
