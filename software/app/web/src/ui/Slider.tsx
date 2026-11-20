import { useEffect, useState } from 'react';
import s from './ui.module.css';

interface Props {
  value: number;
  min: number;
  max: number;
  step?: number;
  label: string;
  /** Called on release (pointer up, key up, blur) with the final value, not for every drag step. */
  onCommit(v: number): void;
}

export function Slider({ value, min, max, step = 1, label, onCommit }: Props) {
  const [local, setLocal] = useState(value);
  useEffect(() => setLocal(value), [value]);
  const commit = () => {
    if (local !== value) onCommit(local);
  };
  return (
    <span className={s.slider}>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={local}
        onChange={(e) => setLocal(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className={s.sliderValue}>{local}</span>
    </span>
  );
}
