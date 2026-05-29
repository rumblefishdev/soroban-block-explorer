import ArrowForwardIcon from '@mui/icons-material/ArrowForward';
import { Box, Link, Typography } from '@mui/material';
import { Link as RouterLink } from 'react-router-dom';

interface ViewAllLinkProps {
  /** Destination route for the full list. */
  to: string;
}

/**
 * "View All →" link with the Figma yellow arrow-circle. Rendered in the
 * action slot of a home-page table section header.
 */
export function ViewAllLink({ to }: ViewAllLinkProps) {
  return (
    <Link
      component={RouterLink}
      to={to}
      underline="none"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        gap: 1,
        color: theme.palette.text.primary,
        '&:hover .ViewAllLink-arrow': {
          backgroundColor: theme.palette.surface.primaryHover,
        },
        '&:focus-visible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: 2,
          borderRadius: 1,
        },
      })}
    >
      <Typography component="span" variant="bodySmMedium">
        View All
      </Typography>
      <Box
        className="ViewAllLink-arrow"
        sx={(theme) => ({
          width: 24,
          height: 24,
          borderRadius: '50%',
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: theme.palette.surface.primaryMain,
          color: 'common.black',
        })}
      >
        <ArrowForwardIcon sx={{ fontSize: 14 }} />
      </Box>
    </Link>
  );
}
