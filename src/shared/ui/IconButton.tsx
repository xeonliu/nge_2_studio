import type { ButtonHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: LucideIcon;
  label: string;
}

export function IconButton({ icon: Icon, label, className = "", ...props }: IconButtonProps) {
  return (
    <button className={`icon-button ${className}`} type="button" title={label} aria-label={label} {...props}>
      <Icon size={16} strokeWidth={1.8} aria-hidden="true" />
    </button>
  );
}

