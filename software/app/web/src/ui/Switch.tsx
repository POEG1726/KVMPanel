import s from './ui.module.css';

interface Props {
  checked: boolean;
  onChange(next: boolean): void;
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled }: Props) {
  return <button type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} className={s.switch} onClick={() => onChange(!checked)} />;
}
