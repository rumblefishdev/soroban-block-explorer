import { Breadcrumbs, Link, Typography } from '@mui/material';
import { Link as RouterLink } from 'react-router-dom';

export interface BreadcrumbItem {
  label: string;
  /** Route to link to. Omit for the current (last) crumb. */
  to?: string;
}

/**
 * Breadcrumb trail shown above a detail page title, e.g.
 * `Account / GDQP…EE36`. The final crumb is rendered as plain text.
 */
export function PageBreadcrumb({ items }: { items: BreadcrumbItem[] }) {
  return (
    <Breadcrumbs aria-label="Breadcrumb" sx={{ mb: 1 }}>
      {items.map((item, index) => {
        const isLast = index === items.length - 1;
        if (isLast || !item.to) {
          return (
            <Typography
              key={index}
              variant="bodySmMedium"
              sx={(theme) => ({ color: theme.palette.text.primary })}
            >
              {item.label}
            </Typography>
          );
        }
        return (
          <Link
            key={index}
            component={RouterLink}
            to={item.to}
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            {item.label}
          </Link>
        );
      })}
    </Breadcrumbs>
  );
}
