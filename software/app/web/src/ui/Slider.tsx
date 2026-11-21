import { useEffect, useRef, useState } from 'react';
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
  const changed = useRef(false);
  useEffect(() => { setLocal(value); changed.current = false; }, [value]);
  const commit = () => {
    if (changed.current && local !== value) onCommit(local);
    changed.current = false;
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
        onChange={(e) => { changed.current = true; setLocal(Number(e.target.value)); }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className={s.sliderValue}>{local}</span>
    </span>
  );
}
