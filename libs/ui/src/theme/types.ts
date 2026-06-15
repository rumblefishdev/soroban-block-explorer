import type { CSSProperties } from 'react';

import type { scales } from './colors.js';
import type { radius } from './radius.js';

interface TypeSurface {
  background: string;
  backgroundAlt: string;
  information: string;
  success: string;
  warning: string;
  error: string;
  primaryMain: string;
  primaryMainAlt: string;
  primaryHover: string;
  primaryPressed: string;
  primaryDisabled: string;
  primaryLight: string;
  grayMain: string;
  grayMainAlt: string;
  grayHover: string;
  grayPressed: string;
  grayDisabled: string;
  grayLight: string;
  grayInverted: string;
}

interface TypeStroke {
  default: string;
  defaultHover: string;
  action: string;
  actionHover: string;
  success: string;
  warning: string;
  error: string;
}

interface ExplorerHeadingVariants {
  heading1Bold: CSSProperties;
  heading1SemiBold: CSSProperties;
  heading1Medium: CSSProperties;
  heading1Regular: CSSProperties;
  heading2Bold: CSSProperties;
  heading2SemiBold: CSSProperties;
  heading2Medium: CSSProperties;
  heading2Regular: CSSProperties;
  heading3Bold: CSSProperties;
  heading3SemiBold: CSSProperties;
  heading3Medium: CSSProperties;
  heading3Regular: CSSProperties;
  heading4Bold: CSSProperties;
  heading4SemiBold: CSSProperties;
  heading4Medium: CSSProperties;
  heading4Regular: CSSProperties;
  heading5Bold: CSSProperties;
  heading5SemiBold: CSSProperties;
  heading5Medium: CSSProperties;
  heading5Regular: CSSProperties;
  heading6Bold: CSSProperties;
  heading6SemiBold: CSSProperties;
  heading6Medium: CSSProperties;
  heading6Regular: CSSProperties;
}

interface ExplorerBodyVariants {
  bodyLgBold: CSSProperties;
  bodyLgMedium: CSSProperties;
  bodyLgRegular: CSSProperties;
  bodyMdBold: CSSProperties;
  bodyMdMedium: CSSProperties;
  bodyMdRegular: CSSProperties;
  bodyBold: CSSProperties;
  bodyMedium: CSSProperties;
  bodyRegular: CSSProperties;
  bodySmBold: CSSProperties;
  bodySmSemiBold: CSSProperties;
  bodySmMedium: CSSProperties;
  bodySmRegular: CSSProperties;
  bodyXsBold: CSSProperties;
  bodyXsMedium: CSSProperties;
  bodyXsRegular: CSSProperties;
}

interface ExplorerMonoVariants {
  bodyMonoLgBold: CSSProperties;
  bodyMonoLgRegular: CSSProperties;
  bodyMonoMdBold: CSSProperties;
  bodyMonoMdRegular: CSSProperties;
  bodyMonoBold: CSSProperties;
  bodyMonoRegular: CSSProperties;
  bodyMonoSmBold: CSSProperties;
  bodyMonoSmMedium: CSSProperties;
  bodyMonoSmRegular: CSSProperties;
  bodyMonoXsBold: CSSProperties;
  bodyMonoXsMedium: CSSProperties;
  bodyMonoXsRegular: CSSProperties;
}

declare module '@mui/material/styles' {
  interface TypeText {
    tertiary: string;
    inverted: string;
    accent: string;
    success: string;
    warning: string;
    error: string;
  }

  interface Palette {
    surface: TypeSurface;
    stroke: TypeStroke;
    base: typeof scales.base;
    gray: typeof scales.gray;
    green: typeof scales.green;
    red: typeof scales.red;
    yellow: typeof scales.yellow;
    blue: typeof scales.blue;
    violet: typeof scales.violet;
    emerald: typeof scales.emerald;
  }

  interface PaletteOptions {
    surface?: Partial<TypeSurface>;
    stroke?: Partial<TypeStroke>;
    base?: typeof scales.base;
    gray?: typeof scales.gray;
    green?: typeof scales.green;
    red?: typeof scales.red;
    yellow?: typeof scales.yellow;
    blue?: typeof scales.blue;
    violet?: typeof scales.violet;
    emerald?: typeof scales.emerald;
  }

  interface TypographyVariants
    extends ExplorerHeadingVariants,
      ExplorerBodyVariants,
      ExplorerMonoVariants {}

  interface TypographyVariantsOptions
    extends Partial<ExplorerHeadingVariants>,
      Partial<ExplorerBodyVariants>,
      Partial<ExplorerMonoVariants> {}

  interface Shape {
    radius: typeof radius;
  }

  interface ShapeOptions {
    radius?: typeof radius;
  }

  interface ZIndex {
    pageGlow: number;
    gridBackdrop: number;
    contentMain: number;
    secondaryNav: number;
    footer: number;
    topNav: number;
  }
}

declare module '@mui/material/Switch' {
  interface SwitchPropsSizeOverrides {
    lg: true;
    md: true;
    sm: true;
  }
}

declare module '@mui/material/Checkbox' {
  interface CheckboxPropsSizeOverrides {
    lg: true;
    md: true;
    sm: true;
  }
}

declare module '@mui/material/Radio' {
  interface RadioPropsSizeOverrides {
    lg: true;
    md: true;
    sm: true;
  }
}

declare module '@mui/material/Chip' {
  interface ChipPropsColorOverrides {
    blue: true;
    violet: true;
    emerald: true;
    neutral: true;
    subtle: true;
    brown: true;
    accent: true;
  }

  interface ChipPropsSizeOverrides {
    lg: true;
    md: true;
    sm: true;
  }
}

declare module '@mui/material/Typography' {
  interface TypographyPropsVariantOverrides {
    heading1Bold: true;
    heading1SemiBold: true;
    heading1Medium: true;
    heading1Regular: true;
    heading2Bold: true;
    heading2SemiBold: true;
    heading2Medium: true;
    heading2Regular: true;
    heading3Bold: true;
    heading3SemiBold: true;
    heading3Medium: true;
    heading3Regular: true;
    heading4Bold: true;
    heading4SemiBold: true;
    heading4Medium: true;
    heading4Regular: true;
    heading5Bold: true;
    heading5SemiBold: true;
    heading5Medium: true;
    heading5Regular: true;
    heading6Bold: true;
    heading6SemiBold: true;
    heading6Medium: true;
    heading6Regular: true;
    bodyLgBold: true;
    bodyLgMedium: true;
    bodyLgRegular: true;
    bodyMdBold: true;
    bodyMdMedium: true;
    bodyMdRegular: true;
    bodyBold: true;
    bodyMedium: true;
    bodyRegular: true;
    bodySmBold: true;
    bodySmSemiBold: true;
    bodySmMedium: true;
    bodySmRegular: true;
    bodyXsBold: true;
    bodyXsMedium: true;
    bodyXsRegular: true;
    bodyMonoLgBold: true;
    bodyMonoLgRegular: true;
    bodyMonoMdBold: true;
    bodyMonoMdRegular: true;
    bodyMonoBold: true;
    bodyMonoRegular: true;
    bodyMonoSmBold: true;
    bodyMonoSmMedium: true;
    bodyMonoSmRegular: true;
    bodyMonoXsBold: true;
    bodyMonoXsMedium: true;
    bodyMonoXsRegular: true;
  }
}
