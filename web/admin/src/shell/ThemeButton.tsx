// The theme switch in the header: an icon for the chosen theme (a half circle for System, a sun
// for Light, a moon for Dark); each click moves to the next one, System → Light → Dark → System.

import type { ReactNode } from "react";
import type { Theme } from "../shared/preferences";

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" aria-hidden="true">
      {children}
    </svg>
  );
}

const SUN_RAYS = ["M12 2v2", "M12 20v2", "M2 12h2", "M20 12h2", "M4.9 4.9l1.4 1.4", "M17.7 17.7l1.4 1.4", "M4.9 19.1l1.4-1.4", "M17.7 6.3l1.4-1.4"];

const THEMES: Record<Theme, { label: string; next: Theme; icon: ReactNode }> = {
  system: {
    label: "System",
    next: "light",
    icon: (
      <Icon>
        <circle cx="12" cy="12" r="9" />
        <path d="M12 3a9 9 0 0 0 0 18z" fill="currentColor" />
      </Icon>
    ),
  },
  light: {
    label: "Light",
    next: "dark",
    icon: (
      <Icon>
        <circle cx="12" cy="12" r="4" />
        {SUN_RAYS.map((ray) => (
          <path key={ray} d={ray} />
        ))}
      </Icon>
    ),
  },
  dark: {
    label: "Dark",
    next: "system",
    icon: (
      <Icon>
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
      </Icon>
    ),
  },
};

export function ThemeButton({ theme, onChange }: { theme: Theme; onChange: (theme: Theme) => void }) {
  const { label, next, icon } = THEMES[theme];
  return (
    <button
      type="button"
      className="icon-button"
      aria-label={`Theme: ${label}`}
      title={`Theme: ${label} (click for ${THEMES[next].label})`}
      onClick={() => onChange(next)}
    >
      {icon}
    </button>
  );
}
