import { forwardRef, type ButtonHTMLAttributes } from 'react';
import s from './ui.module.css';

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'default' | 'primary' | 'danger';
}

export const Button = forwardRef<HTMLButtonElement, Props>(function Button({ variant = 'default', className, type = 'button', ...rest }, ref) {
  const cls = [s.button, variant === 'primary' && s.primary, variant === 'danger' && s.danger, className].filter(Boolean).join(' ');
  return <button ref={ref} type={type} className={cls} {...rest} />;
});
