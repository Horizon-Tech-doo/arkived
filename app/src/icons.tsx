// Arkived — Icons (ported from design/icons.jsx)
import React, { CSSProperties, ReactNode } from "react";

export interface IconProps {
  size?: number;
  style?: CSSProperties;
  [key: string]: unknown;
}

interface IProps extends IconProps {
  children: ReactNode;
}

export const I = ({ size = 14, children, style = {}, ...rest }: IProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 14 14"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.3"
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0, ...style }}
    {...rest}
  >
    {children}
  </svg>
);

// Filesystem
export const IconFolder = (p: IconProps) => (
  <I {...p}>
    <path d="M1.5 3.5v7a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1H7L5.5 2.5H2.5a1 1 0 0 0-1 1z" />
  </I>
);
export const IconFolderOpen = (p: IconProps) => (
  <I {...p}>
    <path d="M1.5 11.5V3.5a1 1 0 0 1 1-1h3L7 4h4.5a1 1 0 0 1 1 1v1.5M1.5 11.5l1.6-4.5a1 1 0 0 1 .95-.7h8.5a1 1 0 0 1 .95 1.3l-1.2 3.4a1 1 0 0 1-.95.7H2.5a1 1 0 0 1-1-1z" />
  </I>
);
export const IconFile = (p: IconProps) => (
  <I {...p}>
    <path d="M3 1.5h5L11 4.5v7a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
    <path d="M8 1.5v3h3" />
  </I>
);
export const IconFileCode = (p: IconProps) => (
  <I {...p}>
    <path d="M3 1.5h5L11 4.5v7a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
    <path d="M8 1.5v3h3" />
    <path d="M5.5 8l-1 1.5 1 1.5M8.5 8l1 1.5-1 1.5" />
  </I>
);
export const IconFileArchive = (p: IconProps) => (
  <I {...p}>
    <path d="M3 1.5h5L11 4.5v7a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
    <path d="M6.5 2v1M6.5 4v1M6.5 6v1M6 8.5h1v2H6z" />
  </I>
);
export const IconFileImage = (p: IconProps) => (
  <I {...p}>
    <path d="M3 1.5h5L11 4.5v7a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
    <path d="M8 1.5v3h3" />
    <circle cx="5.5" cy="8" r="0.8" />
    <path d="M3 11l2-2 3 2.5" />
  </I>
);

// Database / storage
export const IconDatabase = (p: IconProps) => (
  <I {...p}>
    <ellipse cx="7" cy="3" rx="4.5" ry="1.5" />
    <path d="M2.5 3v8c0 .83 2 1.5 4.5 1.5s4.5-.67 4.5-1.5V3" />
    <path d="M2.5 7c0 .83 2 1.5 4.5 1.5s4.5-.67 4.5-1.5" />
  </I>
);
export const IconContainer = (p: IconProps) => (
  <I {...p}>
    <rect x="1.5" y="3" width="11" height="8" rx="1" />
    <path d="M1.5 5.5h11M4 3v-1M10 3v-1" />
  </I>
);
export const IconQueue = (p: IconProps) => (
  <I {...p}>
    <rect x="1.5" y="3.5" width="11" height="7" rx="1" />
    <path d="M4 3.5V2M7 3.5V2M10 3.5V2M4 7h6" />
  </I>
);
export const IconTable = (p: IconProps) => (
  <I {...p}>
    <rect x="1.5" y="2" width="11" height="10" rx="1" />
    <path d="M1.5 5.5h11M1.5 8.5h11M5 2v10M9 2v10" />
  </I>
);
export const IconShare = (p: IconProps) => (
  <I {...p}>
    <path d="M1.5 3.5v7a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1H7L5.5 2.5H2.5a1 1 0 0 0-1 1z" />
    <path d="M4 7.5h6M7 5.5v4M9 6l1 1.5-1 1.5" />
  </I>
);

// Cloud
export const IconCloud = (p: IconProps) => (
  <I {...p}>
    <path d="M4 10.5h6a2.5 2.5 0 0 0 .3-4.98A3.5 3.5 0 0 0 3.8 6.5 2 2 0 0 0 4 10.5z" />
  </I>
);
export const IconCloudUp = (p: IconProps) => (
  <I {...p}>
    <path d="M4 10.5h6a2.5 2.5 0 0 0 .3-4.98A3.5 3.5 0 0 0 3.8 6.5 2 2 0 0 0 4 10.5z" />
    <path d="M7 8.5v4M5.5 10L7 8.5l1.5 1.5" />
  </I>
);

// Chevrons
export const IconChevronRight = (p: IconProps) => (
  <I {...p}>
    <path d="M5 3l4 4-4 4" />
  </I>
);
export const IconChevronDown = (p: IconProps) => (
  <I {...p}>
    <path d="M3 5l4 4 4-4" />
  </I>
);
export const IconChevronLeft = (p: IconProps) => (
  <I {...p}>
    <path d="M9 3l-4 4 4 4" />
  </I>
);
export const IconChevronUp = (p: IconProps) => (
  <I {...p}>
    <path d="M3 9l4-4 4 4" />
  </I>
);
export const IconCaretDown = (p: IconProps) => (
  <I {...p}>
    <path d="M3.5 5.5l3.5 3 3.5-3" strokeWidth="1.5" />
  </I>
);

// Actions
export const IconPlus = (p: IconProps) => (
  <I {...p}>
    <path d="M7 2.5v9M2.5 7h9" />
  </I>
);
export const IconMinus = (p: IconProps) => (
  <I {...p}>
    <path d="M2.5 7h9" />
  </I>
);
export const IconX = (p: IconProps) => (
  <I {...p}>
    <path d="M3 3l8 8M11 3l-8 8" />
  </I>
);
export const IconCheck = (p: IconProps) => (
  <I {...p}>
    <path d="M2.5 7.5l3 3 6-7" />
  </I>
);
export const IconSearch = (p: IconProps) => (
  <I {...p}>
    <circle cx="6" cy="6" r="4" />
    <path d="M9 9l3 3" />
  </I>
);
export const IconUpload = (p: IconProps) => (
  <I {...p}>
    <path d="M7 10V2.5M4 5.5L7 2.5l3 3M2.5 11.5h9" />
  </I>
);
export const IconDownload = (p: IconProps) => (
  <I {...p}>
    <path d="M7 2.5v7.5M4 7l3 3 3-3M2.5 11.5h9" />
  </I>
);
export const IconCopy = (p: IconProps) => (
  <I {...p}>
    <rect x="4" y="4" width="8" height="8" rx="1" />
    <path d="M10 4V3a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h1" />
  </I>
);
export const IconTrash = (p: IconProps) => (
  <I {...p}>
    <path d="M2.5 3.5h9M5 3.5V2.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1M3.5 3.5v8a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1v-8M6 6v4M8 6v4" />
  </I>
);
export const IconRefresh = (p: IconProps) => (
  <I {...p}>
    <path d="M11.5 3v3h-3M2.5 11v-3h3M10.5 5.5a4 4 0 0 0-7-1M3.5 8.5a4 4 0 0 0 7 1" />
  </I>
);
export const IconMore = (p: IconProps) => (
  <I {...p}>
    <circle cx="3" cy="7" r="0.8" fill="currentColor" />
    <circle cx="7" cy="7" r="0.8" fill="currentColor" />
    <circle cx="11" cy="7" r="0.8" fill="currentColor" />
  </I>
);
export const IconSettings = (p: IconProps) => (
  <I {...p}>
    <circle cx="7" cy="7" r="2" />
    <path d="M7 1v2M7 11v2M1 7h2M11 7h2M2.8 2.8l1.4 1.4M9.8 9.8l1.4 1.4M2.8 11.2l1.4-1.4M9.8 4.2l1.4-1.4" />
  </I>
);
export const IconFilter = (p: IconProps) => (
  <I {...p}>
    <path d="M1.5 2.5h11L8.5 7v4.5L5.5 10V7z" />
  </I>
);
export const IconEye = (p: IconProps) => (
  <I {...p}>
    <path d="M1 7s2-4 6-4 6 4 6 4-2 4-6 4-6-4-6-4z" />
    <circle cx="7" cy="7" r="1.5" />
  </I>
);
export const IconPencil = (p: IconProps) => (
  <I {...p}>
    <path d="M10 2.5l1.5 1.5-7 7H3v-1.5z" />
  </I>
);
export const IconLock = (p: IconProps) => (
  <I {...p}>
    <rect x="2.5" y="6" width="9" height="6" rx="1" />
    <path d="M4.5 6V4a2.5 2.5 0 0 1 5 0v2" />
  </I>
);
export const IconUnlock = (p: IconProps) => (
  <I {...p}>
    <rect x="2.5" y="6" width="9" height="6" rx="1" />
    <path d="M4.5 6V4a2.5 2.5 0 0 1 4.5-1.5" />
  </I>
);
export const IconKey = (p: IconProps) => (
  <I {...p}>
    <circle cx="4" cy="7" r="2.5" />
    <path d="M6.5 7h5.5v2h-1.5M10 7v2" />
  </I>
);
export const IconUser = (p: IconProps) => (
  <I {...p}>
    <circle cx="7" cy="4.2" r="2.2" />
    <path d="M2.8 11.2c.8-2 2.35-3 4.2-3 1.85 0 3.4 1 4.2 3" />
  </I>
);
export const IconPlug = (p: IconProps) => (
  <I {...p}>
    <path d="M5 1.8v3.6M9 1.8v3.6M4 5.4h6v1.1A2.9 2.9 0 0 1 7.6 9.4v2.8H6.4V9.4A2.9 2.9 0 0 1 4 6.5z" />
  </I>
);

// Agent
export const IconSparkle = (p: IconProps) => (
  <I {...p}>
    <path d="M7 1.5l1.2 3.3L11.5 6l-3.3 1.2L7 10.5 5.8 7.2 2.5 6l3.3-1.2zM11.5 10l.6 1.4 1.4.6-1.4.6-.6 1.4-.6-1.4-1.4-.6 1.4-.6z" />
  </I>
);
export const IconTerminal = (p: IconProps) => (
  <I {...p}>
    <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
    <path d="M3.5 5.5l2 1.5-2 1.5M7 8.5h3" />
  </I>
);
export const IconCommand = (p: IconProps) => (
  <I {...p}>
    <path d="M4 2.5a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-3 0 1.5 1.5 0 0 1 1.5-1.5h6A1.5 1.5 0 0 1 11.5 10a1.5 1.5 0 0 1-3 0V4a1.5 1.5 0 0 1 3 0 1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 4 1.5 1.5 0 0 1 4 2.5z" />
  </I>
);
export const IconZap = (p: IconProps) => (
  <I {...p}>
    <path d="M8 1.5L3 8h3.5L6 12.5l5-6.5H7.5z" />
  </I>
);
export const IconBolt = IconZap;
export const IconShield = (p: IconProps) => (
  <I {...p}>
    <path d="M7 1.5l5 2v4c0 3-2 4.5-5 5-3-0.5-5-2-5-5v-4z" />
  </I>
);
export const IconShieldCheck = (p: IconProps) => (
  <I {...p}>
    <path d="M7 1.5l5 2v4c0 3-2 4.5-5 5-3-0.5-5-2-5-5v-4z" />
    <path d="M4.5 7l1.5 1.5 3-3" />
  </I>
);

// Status
export const IconAlert = (p: IconProps) => (
  <I {...p}>
    <path d="M7 1.5l6 11h-12z" />
    <path d="M7 5.5v3M7 10v.5" />
  </I>
);
export const IconInfo = (p: IconProps) => (
  <I {...p}>
    <circle cx="7" cy="7" r="5.5" />
    <path d="M7 6.5v3M7 4.5v0.5" />
  </I>
);
export const IconCircle = (p: IconProps) => (
  <I {...p}>
    <circle cx="7" cy="7" r="5.5" />
  </I>
);

interface IconCircleFilledProps {
  size?: number;
  color?: string;
  style?: CSSProperties;
}
export const IconCircleFilled = ({ size = 8, color = "currentColor", style = {} }: IconCircleFilledProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 8 8"
    style={{ display: "block", flexShrink: 0, ...style }}
  >
    <circle cx="4" cy="4" r="4" fill={color} />
  </svg>
);

export const IconLoader = (p: IconProps) => (
  <I {...p} style={{ animation: "arkived-spin 1s linear infinite", ...(p.style || {}) }}>
    <path
      d="M7 1.5v2M7 10.5v2M1.5 7h2M10.5 7h2M3 3l1.4 1.4M9.6 9.6L11 11M3 11l1.4-1.4M9.6 4.4L11 3"
      strokeLinecap="round"
    />
  </I>
);

// Arrows / Nav
export const IconArrowLeft = (p: IconProps) => (
  <I {...p}>
    <path d="M11.5 7H2.5M6 3.5L2.5 7 6 10.5" />
  </I>
);
export const IconArrowRight = (p: IconProps) => (
  <I {...p}>
    <path d="M2.5 7h9M8 3.5L11.5 7 8 10.5" />
  </I>
);
export const IconArrowUp = (p: IconProps) => (
  <I {...p}>
    <path d="M7 11.5v-9M3.5 6L7 2.5 10.5 6" />
  </I>
);
export const IconExternal = (p: IconProps) => (
  <I {...p}>
    <path d="M5 2.5H2.5v9h9V9M7 7l4.5-4.5M7.5 2.5h4v4" />
  </I>
);

// Brand — Arkived mark
interface IconLogoProps {
  size?: number;
  color?: string;
}
export const IconLogo = ({ size = 16, color = "currentColor" }: IconLogoProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" style={{ display: "block" }}>
    <path d="M3 2h2.5v2H4v8h1.5v2H3zM13 2h-2.5v2H12v8h-1.5v2H13z" fill={color} />
    <path d="M7 5h2v6H7z" fill={color} />
    <path d="M6 4h4v1H6zM6 11h4v1H6z" fill={color} opacity="0.6" />
  </svg>
);

interface IconAzureProps {
  size?: number;
}
export const IconAzure = ({ size = 12 }: IconAzureProps) => (
  <svg width={size} height={size} viewBox="0 0 12 12" fill="none" style={{ display: "block" }}>
    <path d="M5.5 1L1 10.5h3.5l4-6zM7 3.5L11 10.5H4z" fill="#0078d4" />
  </svg>
);
