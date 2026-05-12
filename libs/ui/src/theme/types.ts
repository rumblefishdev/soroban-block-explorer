import type { CSSProperties } from 'react';

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
  bodySmMedium: CSSProperties;
  bodySmRegular: CSSProperties;
  bodyXsBold: CSSProperties;
  bodyXsMedium: CSSProperties;
  bodyXsRegular: CSSProperties;
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
  }

  interface PaletteOptions {
    surface?: Partial<TypeSurface>;
    stroke?: Partial<TypeStroke>;
  }

  interface TypographyVariants
    extends ExplorerHeadingVariants,
      ExplorerBodyVariants {}

  interface TypographyVariantsOptions
    extends Partial<ExplorerHeadingVariants>,
      Partial<ExplorerBodyVariants> {}

  interface Shape {
    radius: typeof radius;
  }

  interface ShapeOptions {
    radius?: typeof radius;
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
    bodySmMedium: true;
    bodySmRegular: true;
    bodyXsBold: true;
    bodyXsMedium: true;
    bodyXsRegular: true;
  }
}
