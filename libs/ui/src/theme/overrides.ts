import { alpha, type Components, type Theme } from '@mui/material/styles';

import { scales } from './colors.js';
import { secondaryFontFamily } from './typography.js';

export const overrides: Components<Theme> = {
  MuiCssBaseline: {
    styleOverrides: (theme) => ({
      '*': {
        scrollbarWidth: 'thin',
        scrollbarColor: `${theme.palette.stroke.default} transparent`,
      },
      '*::-webkit-scrollbar': {
        width: 6,
        height: 6,
      },
      '*::-webkit-scrollbar-track': {
        backgroundColor: 'transparent',
      },
      '*::-webkit-scrollbar-thumb': {
        backgroundColor: theme.palette.stroke.default,
        borderRadius: 3,
      },
      '*::-webkit-scrollbar-thumb:hover': {
        backgroundColor: theme.palette.text.tertiary,
      },
    }),
  },

  MuiButton: {
    defaultProps: {
      disableElevation: true,
    },
    styleOverrides: {
      root: ({ theme }) => ({
        borderRadius: theme.shape.radius.pills,
        textTransform: 'none',
        fontWeight: 500,
        letterSpacing: '-0.02em',

        gap: 8,
        '&.Mui-focusVisible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: 2,
        },
        '& .MuiButton-startIcon, & .MuiButton-endIcon': {
          margin: 0,
          padding: 5,
          borderRadius: theme.shape.radius.pills,
          backgroundColor: theme.palette.surface.primaryMain,
          color: theme.palette.common.black,
          '& > svg': { fontSize: 14, width: 14, height: 14 },
        },
      }),

      sizeSmall: {
        padding: '8px 16px',
        fontSize: 14,
        lineHeight: 1.4,
        '&:has(.MuiButton-startIcon)': { paddingLeft: 8 },
        '&:has(.MuiButton-endIcon)': { paddingRight: 8 },
        '& .MuiButton-startIcon, & .MuiButton-endIcon': {
          padding: 3,
          '& > svg': { fontSize: 12, width: 12, height: 12 },
        },
      },
      sizeMedium: {
        padding: '8px 16px',
        fontSize: 16,
        lineHeight: 1.5,
        '&:has(.MuiButton-startIcon)': { paddingLeft: 8 },
        '&:has(.MuiButton-endIcon)': { paddingRight: 8 },
      },
      sizeLarge: {
        padding: '10px 20px',
        fontSize: 16,
        lineHeight: 1.5,
        '&:has(.MuiButton-startIcon)': { paddingLeft: 10 },
        '&:has(.MuiButton-endIcon)': { paddingRight: 10 },
      },

      containedPrimary: ({ theme }) => ({
        backgroundColor: theme.palette.primary.main,
        color: theme.palette.primary.contrastText,
        '&:hover': {
          backgroundColor: theme.palette.primary.dark,
        },
        '&:active': {
          backgroundColor: theme.palette.primary.dark,
          color: theme.palette.grey[300],
        },
        '&.Mui-disabled': {
          backgroundColor: theme.palette.surface.grayDisabled,
          color: theme.palette.grey[400],
        },
      }),

      containedSecondary: ({ theme }) => ({
        backgroundColor: theme.palette.secondary.main,
        color: theme.palette.secondary.contrastText,
        '&:hover': {
          backgroundColor: theme.palette.secondary.dark,
        },
        '&:active': {
          backgroundColor: theme.palette.secondary.dark,
          // Dark mode: text.secondary (#d3d3d3) is light gray → reads
          // washed-out on the yellow press background. Drop to gray.700
          // so the pressed text stays dark and readable in both modes.
          color:
            theme.palette.mode === 'dark'
              ? scales.gray[700]
              : theme.palette.text.secondary,
        },
        '&.Mui-disabled': {
          backgroundColor: theme.palette.surface.primaryDisabled,
          color: theme.palette.text.secondary,
        },
        '& .MuiButton-startIcon, & .MuiButton-endIcon': {
          // Sits on `common.black` in BOTH modes, so it wants the brand yellow,
          // not `text.accent` (graphite in light mode, for legibility on the
          // page surface).
          backgroundColor: theme.palette.common.black,
          color: theme.palette.surface.primaryMain,
        },
        '&:active .MuiButton-startIcon, &:active .MuiButton-endIcon': {
          // Same mode-aware fix for the icon chip wrapping.
          backgroundColor:
            theme.palette.mode === 'dark'
              ? scales.gray[700]
              : theme.palette.text.secondary,
        },
        '&.Mui-disabled .MuiButton-startIcon, &.Mui-disabled .MuiButton-endIcon':
          {
            backgroundColor: theme.palette.grey[600],
            color: theme.palette.grey[400],
          },
      }),

      outlined: ({ theme }) => ({
        backgroundColor: theme.palette.grey[50],
        color: theme.palette.common.black,
        borderColor: theme.palette.stroke.default,
        '&:hover': {
          backgroundColor: theme.palette.grey[100],
          borderColor: theme.palette.stroke.default,
        },
        '&:active': {
          backgroundColor: theme.palette.grey[100],
          color: theme.palette.grey[700],
        },
        '&.Mui-disabled': {
          backgroundColor: theme.palette.grey[100],
          color: theme.palette.grey[400],
          borderColor: theme.palette.stroke.default,
        },
      }),

      text: ({ theme }) => ({
        backgroundColor: 'transparent',
        color: theme.palette.text.primary,
        '&:hover': {
          backgroundColor: theme.palette.surface.grayHover,
        },
        '&:active': {
          backgroundColor: 'transparent',
          color: theme.palette.text.secondary,
        },
        '&.Mui-disabled': {
          backgroundColor: 'transparent',
          color: theme.palette.text.tertiary,
        },
      }),
    },
  },

  MuiChip: {
    defaultProps: {
      size: 'lg',
    },
    styleOverrides: {
      root: ({ theme }) => ({
        borderRadius: 8,
        fontFamily: secondaryFontFamily,
        fontWeight: 500,
        letterSpacing: '-0.02em',
        height: 'auto',
        gap: '8px',
        '& .MuiChip-icon': {
          marginLeft: 0,
          marginRight: 0,
          color: 'inherit',
        },
        '& .MuiChip-label': {
          paddingLeft: 0,
          paddingRight: 0,
          overflow: 'visible',
        },
        '&.MuiChip-clickable:focus-visible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: 2,
        },
      }),
    },
    variants: [
      {
        props: { size: 'lg' },
        style: { padding: '4px 12px', fontSize: 16, lineHeight: 1.5 },
      },
      {
        props: { size: 'md' },
        style: { padding: '4px 12px', fontSize: 14, lineHeight: 1.4 },
      },
      {
        props: { size: 'sm' },
        style: {
          padding: '2px 8px',
          fontSize: 12,
          fontWeight: 700,
          lineHeight: 1.4,
        },
      },

      {
        props: { color: 'success' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.success,
          color: theme.palette.text.success,
        }),
      },
      {
        props: { color: 'error' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.error,
          color: theme.palette.text.error,
        }),
      },
      {
        props: { color: 'warning' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.warning,
          color: theme.palette.text.warning,
        }),
      },
      {
        props: { color: 'blue' },
        style: { backgroundColor: scales.blue[100], color: scales.blue[600] },
      },
      {
        props: { color: 'violet' },
        style: {
          backgroundColor: scales.violet[100],
          color: scales.violet[600],
        },
      },
      {
        props: { color: 'emerald' },
        style: {
          backgroundColor: scales.emerald[100],
          color: scales.emerald[600],
        },
      },
      {
        props: { color: 'neutral' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.grayLight,
          color: theme.palette.text.primary,
          '&.MuiChip-clickable:hover': {
            backgroundColor: theme.palette.surface.grayHover,
          },
        }),
      },
      {
        props: { color: 'subtle' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.grayMain,
          color: theme.palette.text.primary,
        }),
      },
      {
        props: { color: 'brown' },
        style: {
          backgroundColor: scales.primary[900],
          color: scales.primary[100],
        },
      },
      {
        props: { color: 'accent' },
        style: ({ theme }) => ({
          backgroundColor: theme.palette.surface.primaryMain,
          color: theme.palette.common.black,
          '&.MuiChip-clickable:hover': {
            backgroundColor: theme.palette.surface.primaryHover,
          },
        }),
      },
    ],
  },

  MuiSwitch: {
    defaultProps: { size: 'md' },
    styleOverrides: {
      root: { padding: 0, overflow: 'visible' },
      switchBase: ({ theme }) => ({
        padding: 2,
        color: theme.palette.common.white,
        '&:hover + .MuiSwitch-track': {
          // Light: gray.300 darkens the rest-state track (gray.200) for a
          // visible hover. Dark: gray.500 lightens rest-state (gray.600).
          backgroundColor:
            theme.palette.mode === 'dark' ? scales.gray[500] : scales.gray[300],
        },
        '&:active + .MuiSwitch-track': {
          backgroundColor: theme.palette.surface.grayPressed,
        },
        '&.Mui-disabled + .MuiSwitch-track': {
          backgroundColor: theme.palette.surface.grayLight,
          opacity: 0.5,
        },
        '&.Mui-checked': {
          color: theme.palette.common.white,
          '& + .MuiSwitch-track': {
            backgroundColor: theme.palette.surface.primaryMain,
            opacity: 1,
          },
          '&:hover + .MuiSwitch-track': {
            backgroundColor: theme.palette.surface.primaryHover,
          },
          '&:active + .MuiSwitch-track': {
            backgroundColor: theme.palette.surface.primaryPressed,
          },
          '&.Mui-disabled + .MuiSwitch-track': {
            backgroundColor: theme.palette.surface.primaryDisabled,
            // Light: #fffcc2 is already pale → opacity 1 is fine.
            // Dark:  #ffe945 is vibrant → fade so it reads as disabled.
            opacity: theme.palette.mode === 'dark' ? 0.5 : 1,
          },
        },
      }),
      thumb: {
        backgroundColor: scales.base.white,
        boxShadow: 'none',
      },
      track: ({ theme }) => ({
        borderRadius: 100,
        backgroundColor: theme.palette.surface.grayLight,
        opacity: 1,
        transition: theme.transitions.create(['background-color', 'opacity']),
      }),
    },
    variants: [
      {
        props: { size: 'sm' },
        style: {
          width: 36,
          height: 20,
          '& .MuiSwitch-thumb': { width: 16, height: 16 },
          '& .MuiSwitch-switchBase.Mui-checked': {
            transform: 'translateX(16px)',
          },
        },
      },
      {
        props: { size: 'md' },
        style: {
          width: 40,
          height: 22,
          '& .MuiSwitch-thumb': { width: 18, height: 18 },
          '& .MuiSwitch-switchBase.Mui-checked': {
            transform: 'translateX(18px)',
          },
        },
      },
      {
        props: { size: 'lg' },
        style: {
          width: 44,
          height: 24,
          '& .MuiSwitch-thumb': { width: 20, height: 20 },
          '& .MuiSwitch-switchBase.Mui-checked': {
            transform: 'translateX(20px)',
          },
        },
      },
    ],
  },

  MuiFormControlLabel: {
    styleOverrides: {
      root: {
        gap: 4,
        marginLeft: 0,
      },
    },
  },

  MuiCheckbox: {
    defaultProps: { disableRipple: true },
    styleOverrides: {
      root: ({ theme }) => ({
        padding: 0,
        '& .MuiSvgIcon-root': {
          width: 20,
          height: 20,
          borderRadius: 4,
          color: theme.palette.surface.grayMain,
          border: `1px solid ${theme.palette.grey[200]}`,
          boxSizing: 'border-box',
          transition: theme.transitions.create([
            'color',
            'border-color',
            'background-color',
          ]),
        },
        '&:hover:not(.Mui-checked):not(.MuiCheckbox-indeterminate):not(.Mui-disabled) .MuiSvgIcon-root':
          {
            borderColor: theme.palette.grey[400],
            backgroundColor: theme.palette.surface.grayHover,
          },
        '&:active:not(.Mui-checked):not(.MuiCheckbox-indeterminate):not(.Mui-disabled) .MuiSvgIcon-root':
          {
            backgroundColor: theme.palette.surface.grayPressed,
          },
        '&.Mui-checked, &.MuiCheckbox-indeterminate': {
          '& .MuiSvgIcon-root': {
            color: theme.palette.surface.primaryMain,
            border: 'none',
          },
          '&:hover:not(.Mui-disabled) .MuiSvgIcon-root': {
            color: theme.palette.surface.primaryHover,
          },
          '&:active:not(.Mui-disabled) .MuiSvgIcon-root': {
            color: theme.palette.surface.primaryPressed,
          },
        },
        '&.Mui-disabled .MuiSvgIcon-root': {
          opacity: 0.4,
        },
      }),
    },
  },

  MuiSlider: {
    styleOverrides: {
      root: ({ theme }) => ({
        height: 4,
        color: theme.palette.surface.primaryMain,
        padding: '13px 0',
        '&.Mui-disabled': {
          opacity: 0.5,
        },
      }),
      rail: ({ theme }) => ({
        height: 4,
        opacity: 1,
        backgroundColor: theme.palette.surface.grayLight,
      }),
      track: ({ theme }) => ({
        height: 4,
        border: 'none',
        backgroundColor: theme.palette.surface.primaryMain,
      }),
      thumb: ({ theme }) => ({
        width: 18,
        height: 18,
        backgroundColor: theme.palette.common.white,
        border: `2px solid ${theme.palette.surface.primaryMain}`,
        boxShadow: theme.shadows[1],
        '&:hover, &.Mui-focusVisible': {
          boxShadow: theme.shadows[3],
        },
        '&:active': {
          boxShadow: theme.shadows[5],
        },
        '&:before': {
          display: 'none',
        },
      }),
      valueLabel: ({ theme }) => ({
        position: 'absolute',
        left: '50%',
        right: 'auto',
        top: -10,
        transformOrigin: 'center bottom',
        '&, &.MuiSlider-valueLabelOpen': {
          transform: 'translate(-50%, -100%) scale(1)',
        },

        backgroundColor: theme.palette.surface.grayInverted,
        color: theme.palette.text.inverted,
        fontFamily: secondaryFontFamily,
        fontSize: 12,
        fontWeight: 600,
        lineHeight: 1.4,
        letterSpacing: '-0.02em',
        padding: '4px 8px',
        borderRadius: 8,
        boxShadow: theme.shadows[3],
        '&:before': {
          display: 'none',
        },
        '& *': {
          background: 'transparent',
          color: theme.palette.text.inverted,
        },
      }),
      mark: ({ theme }) => ({
        backgroundColor: theme.palette.surface.grayLight,
        width: 2,
        height: 8,
      }),
      markActive: ({ theme }) => ({
        backgroundColor: theme.palette.surface.primaryHover,
      }),
    },
  },

  MuiTextField: {
    defaultProps: {
      variant: 'outlined',
    },
  },
  MuiOutlinedInput: {
    styleOverrides: {
      root: ({ theme }) => ({
        backgroundColor: theme.palette.surface.grayMain,
        borderRadius: 8,
        fontFamily: secondaryFontFamily,
        fontSize: 14,
        letterSpacing: '-0.02em',
        transition: theme.transitions.create(['background-color']),

        '& .MuiOutlinedInput-notchedOutline': {
          borderColor: theme.palette.stroke.default,
          transition: 'border-color 0.15s, border-width 0.15s',
        },

        '&:hover:not(.Mui-disabled):not(.Mui-focused):not(.Mui-error) .MuiOutlinedInput-notchedOutline':
          {
            borderColor: theme.palette.stroke.defaultHover,
          },

        '&.Mui-focused .MuiOutlinedInput-notchedOutline': {
          borderColor: theme.palette.stroke.action,
          borderWidth: 2,
        },

        '&.Mui-error .MuiOutlinedInput-notchedOutline': {
          borderColor: theme.palette.stroke.error,
        },
        '&.Mui-error.Mui-focused .MuiOutlinedInput-notchedOutline': {
          borderColor: theme.palette.stroke.error,
          borderWidth: 2,
        },

        '&.Mui-disabled': {
          opacity: 0.5,
          '& .MuiOutlinedInput-notchedOutline': {
            borderColor: theme.palette.stroke.default,
          },
        },
      }),
      input: ({ theme }) => ({
        padding: '10px 14px',
        color: theme.palette.text.primary,
        caretColor: theme.palette.text.accent,
        '&::placeholder': {
          color: theme.palette.text.tertiary,
          opacity: 1,
        },
      }),
      multiline: {
        padding: 0,
      },
    },
  },
  MuiInputLabel: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        fontSize: 14,
        letterSpacing: '-0.02em',
        color: theme.palette.text.primary,
        '&.Mui-focused': {
          color: theme.palette.text.primary,
        },
        '&.Mui-error': {
          color: theme.palette.text.error,
        },
        '&.Mui-disabled': {
          opacity: 0.5,
        },
      }),
    },
  },
  MuiFormHelperText: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        fontSize: 12,
        lineHeight: 1.4,
        letterSpacing: '-0.02em',
        marginLeft: 0,
        marginRight: 0,
        color: theme.palette.text.secondary,
        '&.Mui-error': {
          color: theme.palette.text.error,
        },
        '&.Mui-disabled': {
          opacity: 0.5,
        },
      }),
    },
  },

  MuiSelect: {
    defaultProps: {
      variant: 'outlined',
    },
    styleOverrides: {
      icon: ({ theme }) => ({
        color: theme.palette.text.secondary,
        transition: theme.transitions.create('transform'),
      }),
    },
  },

  MuiMenu: {
    defaultProps: {
      slotProps: {
        paper: {
          elevation: 0,
        },
      },
    },
    styleOverrides: {
      paper: ({ theme }) => ({
        marginTop: 4,
        borderRadius: 8,
        border: `1px solid ${theme.palette.stroke.default}`,
        boxShadow: theme.shadows[3],
        backgroundColor: theme.palette.surface.grayMain,
      }),
      list: {
        padding: 4,
      },
    },
  },

  MuiMenuItem: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        fontSize: 14,
        letterSpacing: '-0.02em',
        color: theme.palette.text.primary,
        borderRadius: 6,
        padding: '8px 12px',
        minHeight: 'unset',
        transition: theme.transitions.create(['background-color', 'color']),
        '&:hover, &.Mui-focusVisible': {
          backgroundColor: alpha(theme.palette.surface.primaryMain, 0.25),
        },
        '&.Mui-selected': {
          backgroundColor: 'transparent',
          fontWeight: 500,
          '&:hover, &.Mui-focusVisible': {
            backgroundColor: alpha(theme.palette.surface.primaryMain, 0.25),
          },
        },
        '&.Mui-disabled': {
          opacity: 0.5,
        },
      }),
    },
  },

  MuiListSubheader: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        fontSize: 12,
        fontWeight: 500,
        letterSpacing: '-0.02em',
        color: theme.palette.text.secondary,
        lineHeight: 1.4,
        padding: '8px 12px 4px',
        backgroundColor: 'transparent',
      }),
    },
  },

  MuiPaper: {
    defaultProps: {
      elevation: 0,
    },
    styleOverrides: {
      root: ({ theme }) => ({
        backgroundColor: theme.palette.surface.grayMain,
        backgroundImage: 'none',
      }),
      outlined: ({ theme }) => ({
        border: `1px solid ${theme.palette.stroke.default}`,
        borderRadius: 16,
      }),
    },
  },

  MuiCard: {
    defaultProps: {
      elevation: 0,
      variant: 'outlined',
    },
    styleOverrides: {
      root: ({ theme }) => ({
        backgroundColor: theme.palette.surface.grayMain,
        border: `1px solid ${theme.palette.stroke.default}`,
        borderRadius: 16,
      }),
    },
  },

  MuiTabs: {
    styleOverrides: {
      root: ({ theme }) => ({
        minHeight: 'unset',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      }),
      indicator: ({ theme }) => ({
        backgroundColor: theme.palette.surface.primaryMain,
        height: 2,
      }),
    },
  },
  MuiTab: {
    styleOverrides: {
      root: ({ theme }) => ({
        textTransform: 'none',
        fontFamily: secondaryFontFamily,
        fontWeight: 500,
        fontSize: 14,
        letterSpacing: '-0.02em',
        color: theme.palette.text.secondary,
        minHeight: 'unset',
        padding: '10px 16px',
        gap: 8,
        // The hover fill and the focus ring need a radius of their own: a
        // tab is a bare padded box, so without one both render as hard
        // rectangles. TOP corners only — a tab sits ON the header's bottom
        // border (and, when selected, on its 2px indicator), so rounding
        // the bottom would lift the fill off that baseline and break the
        // tab metaphor. `radius.s` matches the range pills in the same
        // chart-card header, so one container speaks one corner language.
        borderRadius: `${theme.shape.radius.s}px ${theme.shape.radius.s}px 0 0`,
        transition: theme.transitions.create(['background-color', 'color'], {
          duration: theme.transitions.duration.shorter,
        }),
        '&.Mui-selected': {
          color: theme.palette.text.primary,
          fontWeight: 600,
        },
        '&:hover:not(.Mui-selected):not(.Mui-disabled)': {
          color: theme.palette.text.primary,
          backgroundColor: alpha(theme.palette.surface.primaryMain, 0.15),
        },
        // Keyboard focus had no style at all and fell back to the browser
        // default outline — also a hard rectangle. Same ring idiom as the
        // interval pills.
        '&:focus-visible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: 2,
        },
        '&.Mui-disabled': {
          color: theme.palette.text.tertiary,
        },
      }),
    },
  },

  MuiTableContainer: {
    styleOverrides: {
      root: {
        boxShadow: 'none',
        backgroundColor: 'transparent',
      },
    },
  },
  MuiTable: {
    styleOverrides: {
      root: {
        borderCollapse: 'collapse',
      },
    },
  },
  MuiTableHead: {
    styleOverrides: {
      root: ({ theme }) => ({
        backgroundColor: theme.palette.surface.backgroundAlt,
      }),
    },
  },
  MuiTableRow: {
    styleOverrides: {
      root: ({ theme }) => ({
        '&:hover:not(.MuiTableRow-head)': {
          backgroundColor: alpha(theme.palette.surface.primaryMain, 0.08),
        },
        '&:last-of-type .MuiTableCell-body': {
          borderBottom: 'none',
        },
      }),
    },
  },
  MuiTableCell: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        letterSpacing: '-0.02em',
        padding: '12px 16px',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        verticalAlign: 'middle',
      }),
      head: ({ theme }) => ({
        fontSize: 14,
        fontWeight: 500,
        color: theme.palette.text.primary,
        letterSpacing: '-0.02em',
        padding: '8px 16px',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      }),
      body: ({ theme }) => ({
        fontSize: 14,
        fontWeight: 500,
        color: theme.palette.text.primary,
      }),
    },
  },

  MuiPagination: {
    defaultProps: {
      shape: 'rounded',
      size: 'small',
    },
  },
  MuiPaginationItem: {
    styleOverrides: {
      root: ({ theme }) => ({
        fontFamily: secondaryFontFamily,
        fontSize: 14,
        fontWeight: 500,
        letterSpacing: '-0.02em',
        color: theme.palette.text.primary,
        borderRadius: 8,
        margin: '0 2px',
        minWidth: 32,
        height: 32,
        '&:hover:not(.Mui-disabled):not(.Mui-selected)': {
          backgroundColor: alpha(theme.palette.surface.primaryMain, 0.15),
        },
        '&.Mui-selected': {
          backgroundColor: theme.palette.surface.primaryMain,
          color: theme.palette.common.black,
          '&:hover': {
            backgroundColor: theme.palette.surface.primaryHover,
          },
        },
        '&.Mui-disabled': {
          opacity: 0.5,
        },
      }),
    },
  },

  MuiTooltip: {
    defaultProps: {
      arrow: true,
    },
    styleOverrides: {
      tooltip: ({ theme }) => ({
        backgroundColor: theme.palette.surface.grayInverted,
        color: theme.palette.text.inverted,
        fontFamily: secondaryFontFamily,
        fontSize: 12,
        fontWeight: 500,
        lineHeight: 1.4,
        letterSpacing: '-0.02em',
        padding: '6px 10px',
        borderRadius: 4,
        maxWidth: 320,
        boxShadow: theme.shadows[2],
      }),
      arrow: ({ theme }) => ({
        color: theme.palette.surface.grayInverted,
      }),
    },
  },

  MuiRadio: {
    defaultProps: { disableRipple: true },
    styleOverrides: {
      root: ({ theme }) => ({
        padding: 0,
        '& .MuiSvgIcon-root': {
          width: 20,
          height: 20,
          transition: theme.transitions.create(['color']),
        },
        '&:hover:not(.Mui-checked):not(.Mui-disabled) .MuiSvgIcon-root': {
          color: theme.palette.grey[400],
        },
        '&:active:not(.Mui-checked):not(.Mui-disabled) .MuiSvgIcon-root': {
          color: theme.palette.grey[500],
        },
        '&.Mui-checked .MuiSvgIcon-root': {
          color: theme.palette.surface.primaryMain,
        },
        '&.Mui-checked:hover:not(.Mui-disabled) .MuiSvgIcon-root': {
          color: theme.palette.surface.primaryHover,
        },
        '&.Mui-checked:active:not(.Mui-disabled) .MuiSvgIcon-root': {
          color: theme.palette.surface.primaryPressed,
        },
        '&.Mui-disabled': {
          opacity: 0.4,
        },
      }),
    },
  },
};
