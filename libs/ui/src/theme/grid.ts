export const grid = {
  desktop: {
    columns: 12,
    gutter: 16, // theme.spacing(2)
    margin: 80, // theme.spacing(10) - side padding inside the frame
    maxWidth: 1440, // outer frame width
    contentMaxWidth: 1280, // inside-margins working area (1440 - 2*80)
  },
  mobile: {
    columns: 4,
    gutter: 16, // theme.spacing(2)
    margin: 16, // theme.spacing(2)
    maxWidth: 402, // outer frame width
    contentMaxWidth: 360, // inside-margins working area
  },
} as const;
