export const scales = {
  base: {
    white: '#ffffff',
    black: '#000000',
  },

  gray: {
    50: '#fafafa',
    100: '#f5f5f5',
    200: '#e6e6e6',
    300: '#d3d3d3',
    400: '#a3a3a3',
    500: '#727272',
    600: '#535353',
    700: '#404040',
    800: '#272727',
    900: '#1a1a1a',
    950: '#0f0f0f',
  },

  secondary: {
    50: '#eff0f6',
    100: '#dfe1ec',
    200: '#bec2da',
    300: '#9ea4c7',
    400: '#a4aacb',
    500: '#7d86b5',
    600: '#4a5382',
    700: '#383e61',
    800: '#252941',
    900: '#131520',
  },

  green: {
    50: '#f0fdf4',
    100: '#dcfce7',
    200: '#b9f8cf',
    300: '#7bf1a8',
    400: '#05df72',
    500: '#00c950',
    600: '#00a63e',
    700: '#008236',
    800: '#016630',
    900: '#0d542b',
    950: '#032e15',
  },

  red: {
    50: '#fef2f2',
    100: '#ffe2e2',
    200: '#ffc9c9',
    300: '#ffa2a2',
    400: '#ff6467',
    500: '#fb2c36',
    600: '#e7000b',
    700: '#c10007',
    800: '#9f0712',
    900: '#82181a',
    950: '#460809',
  },

  yellow: {
    50: '#fffbeb',
    100: '#fef3c6',
    200: '#fee685',
    300: '#ffd230',
    400: '#ffb900',
    500: '#fe9a00',
    600: '#e17100',
    700: '#bb4d00',
    800: '#973c00',
    900: '#7b3306',
    950: '#461901',
  },

  blue: { 100: '#dbeafe', 600: '#155dfc', 900: '#1c398e' },
  violet: { 100: '#ede9fe', 600: '#7f22fe', 900: '#4d179a' },
  emerald: { 100: '#d0fae5', 600: '#009966', 900: '#004f3b' },

  primary: { 100: '#fffcc2', 900: '#724311' },
} as const;

export const colorsLight = {
  ...scales,

  text: {
    primary: '#1a1a1a',
    secondary: '#535353',
    tertiary: '#727272',
    inverted: '#fafafa',
    accent: '#fdda24',
    success: '#008236',
    warning: '#bb4d00',
    error: '#c10007',
  },

  surface: {
    background: '#f5f5f5',
    backgroundAlt: '#ffffff',
    information: '#fffcc2',
    success: '#dcfce7',
    warning: '#fef3c6',
    error: '#ffe2e2',
    primaryMain: '#fdda24',
    primaryMainAlt: '#a36905',
    primaryHover: '#edbe05',
    primaryPressed: '#cc9302',
    primaryDisabled: '#fffcc2',
    primaryLight: '#fffcc2',
    grayMain: '#ffffff',
    grayMainAlt: '#fafafa',
    grayHover: '#fafafa',
    grayPressed: '#f5f5f5',
    grayDisabled: '#404040',
    grayLight: '#e6e6e6',
    grayInverted: '#0f0f0f',
  },

  stroke: {
    default: '#e6e6e6',
    defaultHover: '#a3a3a3',
    action: '#fdda24',
    actionHover: '#fff589',
    success: '#00c950',
    warning: '#fe9a00',
    error: '#fb2c36',
  },
} as const;

export const colorsDark = {
  ...scales,

  text: {
    primary: '#f5f5f5',
    secondary: '#d3d3d3',
    tertiary: '#a3a3a3',
    inverted: '#1a1a1a',
    accent: '#fdda24',
    success: '#05df72',
    warning: '#fef3c6',
    error: '#ffa2a2',
  },

  surface: {
    background: '#212121',
    backgroundAlt: '#0f0f0f',
    information: '#724311',
    success: '#0d542b',
    warning: '#bb4d00',
    error: '#82181a',
    primaryMain: '#fdda24',
    primaryMainAlt: '#fdda24',
    primaryHover: '#edbe05',
    primaryPressed: '#a36905',
    primaryDisabled: '#ffe945',
    primaryLight: '#fffcc2',
    grayMain: '#272727',
    grayMainAlt: '#1a1a1a',
    grayHover: '#1a1a1a',
    grayPressed: '#0f0f0f',
    grayDisabled: '#d3d3d3',
    grayLight: '#535353',
    grayInverted: '#fafafa',
  },

  stroke: {
    default: '#535353',
    defaultHover: '#a3a3a3',
    action: '#edbe05',
    actionHover: '#a36905',
    success: '#00c950',
    warning: '#fe9a00',
    error: '#fb2c36',
  },
} as const;

export type ColorScheme = typeof colorsLight;
