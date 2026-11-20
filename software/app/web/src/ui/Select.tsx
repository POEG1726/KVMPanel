import s from './ui.module.css';

interface Props<T extends string> {
  value: T;
  options: readonly { value: T; label: string }[];
  onChange(v: T): void;
  label: string;
  disabled?: boolean;
}

export function Select<T extends string>({ value, options, onChange, label, disabled }: Props<T>) {
  return (
    <select className={s.select} aria-label={label} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value as T)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}
