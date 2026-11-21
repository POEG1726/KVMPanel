import s from './ui.module.css';

interface Props {
  checked: boolean;
  onChange(next: boolean): void;
  label: string;
  disabled?: boolean;
  /** Greyed out but still clickable, so the caller can explain why it cannot be turned on. */
  unavailable?: boolean;
}

export function Switch({ checked, onChange, label, disabled, unavailable }: Props) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-disabled={unavailable || undefined}
      disabled={disabled}
      className={`${s.switch} ${unavailable ? s.switchOff : ''}`}
      onClick={() => onChange(!checked)}
    />
  );
}
