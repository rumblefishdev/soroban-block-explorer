import Box from '@mui/material/Box';
import type { Theme } from '@mui/material';
import Typography from '@mui/material/Typography';

export type NavButtonSize = 'md' | 'lg';

export interface NavButtonProps {
  label: string;
  active?: boolean;
  size?: NavButtonSize;
  badge?: number;
  href?: string;
  onClick?: () => void;
}

const activeSx = {
  position: 'relative',
  color: (theme: Theme) => theme.palette.text.primary,
  '&::after': {
    content: '""',
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
    height: '2px',
    backgroundColor: (theme: Theme) => theme.palette.stroke.action,
    borderRadius: 0,
  },
};

const hoverSx = {
  '&:hover': {
    backgroundColor: (theme: Theme) => theme.palette.surface.background,
    borderRadius: (theme: Theme) => `${theme.shape.radius.s}px`,
    color: (theme: Theme) => theme.palette.text.secondary,
  },
};

const defaultSx = {
  color: (theme: Theme) => theme.palette.text.tertiary,
};

export function NavButton({
  label,
  active = false,
  size = 'md',
  badge,
  href,
  onClick,
}: NavButtonProps) {
  const isLg = size === 'lg';
  const px = 1; // 8px
  const py = isLg ? 1 : 0.5; // 8px / 4px
  const textVariant = isLg ? 'bodyMedium' : 'bodySmMedium';

  const handleClick =
    href && onClick
      ? (e: React.MouseEvent) => {
          if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
          e.preventDefault();
          onClick();
        }
      : onClick;

  return (
    <Box
      component={href ? 'a' : 'button'}
      {...(href ? { href } : { type: 'button' as const })}
      onClick={handleClick}
      sx={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 1,
        px,
        py,
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        textDecoration: 'none',
        fontFamily: 'inherit',
        ...(active ? activeSx : { ...defaultSx, ...hoverSx }),
        transition: 'background-color 0.15s, border-radius 0.15s, color 0.15s',
      }}
    >
      <Box display="inline-flex" alignItems="center">
        <Typography variant={textVariant} color="inherit" noWrap>
          {label}
        </Typography>
      </Box>

      {badge !== undefined && (
        <Box
          sx={(theme) =>
            active
              ? {
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  height: 20,
                  minWidth: 20,
                  px: 0.5,
                  py: 0.25,
                  borderRadius: 9999,
                  backgroundColor: theme.palette.common.black,
                }
              : {
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  height: 20,
                  minWidth: 20,
                  px: 0.5,
                  py: 0.25,
                  borderRadius: 9999,
                  backgroundColor: theme.palette.surface.grayLight,
                }
          }
        >
          <Typography
            variant="bodyXsMedium"
            // Active badge sits on `common.black` in both modes — brand
            // yellow, not `text.accent` (which darkens in light mode).
            color={active ? 'surface.primaryMain' : 'text.tertiary'}
            noWrap
            textAlign="center"
          >
            {badge}
          </Typography>
        </Box>
      )}
    </Box>
  );
}
