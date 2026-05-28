import type { TypographyVariantsOptions } from '@mui/material/styles';

export const headingFontFamily = '"Mona Sans", system-ui, sans-serif';
export const bodyFontFamily = '"Inter", system-ui, sans-serif';
export const monoFontFamily =
  '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

type Weight = 400 | 500 | 600 | 700;

function makeHeading(opts: {
  desktopSize: number;
  mobileSize?: number;
  weight: Weight;
  lineHeight?: number;
}) {
  const { desktopSize, mobileSize, weight, lineHeight = 1.2 } = opts;
  const base = {
    fontFamily: headingFontFamily,
    fontWeight: weight,
    fontSize: desktopSize,
    lineHeight,
    letterSpacing: 0,
  };
  if (mobileSize === undefined || mobileSize === desktopSize) {
    return base;
  }
  return {
    ...base,
    fontSize: mobileSize,
    '@media (min-width: 600px)': { fontSize: desktopSize },
  };
}

function makeBody(size: number, weight: Weight, lineHeight: number) {
  return {
    fontFamily: bodyFontFamily,
    fontSize: size,
    fontWeight: weight,
    lineHeight,
    letterSpacing: '-0.02em',
  };
}

function makeMono(size: number, weight: Weight, lineHeight: number) {
  return {
    fontFamily: monoFontFamily,
    fontSize: size,
    fontWeight: weight,
    lineHeight,
    letterSpacing: 0,
  };
}

export const typography: TypographyVariantsOptions = {
  fontFamily: bodyFontFamily,

  heading1Bold: makeHeading({ desktopSize: 60, weight: 700 }),
  heading1SemiBold: makeHeading({ desktopSize: 60, weight: 600 }),
  heading1Medium: makeHeading({ desktopSize: 60, weight: 500 }),
  heading1Regular: makeHeading({ desktopSize: 60, weight: 400 }),

  heading2Bold: makeHeading({ desktopSize: 48, weight: 700 }),
  heading2SemiBold: makeHeading({ desktopSize: 48, weight: 600 }),
  heading2Medium: makeHeading({ desktopSize: 48, weight: 500 }),
  heading2Regular: makeHeading({ desktopSize: 48, weight: 400 }),

  heading3Bold: makeHeading({ desktopSize: 40, mobileSize: 24, weight: 700 }),
  heading3SemiBold: makeHeading({
    desktopSize: 40,
    mobileSize: 24,
    weight: 600,
  }),
  heading3Medium: makeHeading({ desktopSize: 40, mobileSize: 24, weight: 500 }),
  heading3Regular: makeHeading({
    desktopSize: 40,
    mobileSize: 24,
    weight: 400,
  }),

  heading4Bold: makeHeading({ desktopSize: 32, mobileSize: 20, weight: 700 }),
  heading4SemiBold: makeHeading({
    desktopSize: 32,
    mobileSize: 20,
    weight: 600,
  }),
  heading4Medium: makeHeading({ desktopSize: 32, mobileSize: 20, weight: 500 }),
  heading4Regular: makeHeading({
    desktopSize: 32,
    mobileSize: 20,
    weight: 400,
  }),

  heading5Bold: makeHeading({ desktopSize: 24, mobileSize: 18, weight: 700 }),
  // Figma value: hardcoded 24px regardless of viewport (mirrors source).
  heading5SemiBold: makeHeading({ desktopSize: 24, weight: 600 }),
  heading5Medium: makeHeading({ desktopSize: 24, mobileSize: 18, weight: 500 }),
  heading5Regular: makeHeading({
    desktopSize: 24,
    mobileSize: 18,
    weight: 400,
  }),

  heading6Bold: makeHeading({ desktopSize: 20, mobileSize: 16, weight: 700 }),
  heading6SemiBold: makeHeading({
    desktopSize: 20,
    mobileSize: 16,
    weight: 600,
  }),
  // Figma value: lineHeight 1.22 (outlier vs other h6 weights' 1.2).
  heading6Medium: makeHeading({
    desktopSize: 20,
    mobileSize: 16,
    weight: 500,
    lineHeight: 1.22,
  }),
  heading6Regular: makeHeading({
    desktopSize: 20,
    mobileSize: 16,
    weight: 400,
  }),

  bodyLgBold: makeBody(20, 700, 1.5),
  bodyLgMedium: makeBody(20, 500, 1.5),
  bodyLgRegular: makeBody(20, 400, 1.5),

  bodyMdBold: makeBody(18, 700, 1.5),
  bodyMdMedium: makeBody(18, 500, 1.5),
  bodyMdRegular: makeBody(18, 400, 1.5),

  bodyBold: makeBody(16, 700, 1.5),
  bodyMedium: makeBody(16, 500, 1.5),
  bodyRegular: makeBody(16, 400, 1.5),

  bodySmBold: makeBody(14, 700, 1.4),
  bodySmSemiBold: makeBody(14, 600, 1.4),
  bodySmMedium: makeBody(14, 500, 1.4),
  bodySmRegular: makeBody(14, 400, 1.4),

  bodyXsBold: makeBody(12, 700, 1.4),
  bodyXsMedium: makeBody(12, 500, 1.4),
  bodyXsRegular: makeBody(12, 400, 1.4),

  bodyMonoLgBold: makeMono(20, 700, 1.5),
  bodyMonoLgRegular: makeMono(20, 400, 1.5),
  bodyMonoMdBold: makeMono(18, 700, 1.5),
  bodyMonoMdRegular: makeMono(18, 400, 1.5),
  bodyMonoBold: makeMono(16, 700, 1.5),
  bodyMonoRegular: makeMono(16, 400, 1.5),
  bodyMonoSmBold: makeMono(14, 700, 1.4),
  bodyMonoSmMedium: makeMono(14, 500, 1.4),
  bodyMonoSmRegular: makeMono(14, 400, 1.4),
  bodyMonoXsBold: makeMono(12, 700, 1.4),
  bodyMonoXsRegular: makeMono(12, 400, 1.4),
};
