import type { TypographyOptions } from '@mui/material/styles/createTypography';

const fontFamily = "'Geist', 'Inter', system-ui, -apple-system, sans-serif";

export const typography: TypographyOptions = {
  fontFamily,
  htmlFontSize: 16,
  fontSize: 13,
  h1: {
    fontFamily,
    fontSize: 26,
    fontWeight: 600,
    lineHeight: '32px',
    letterSpacing: '-0.02em',
  },
  h2: {
    fontFamily,
    fontSize: 16,
    fontWeight: 600,
    lineHeight: '24px',
    letterSpacing: '-0.01em',
  },
  h3: { fontFamily, fontSize: 14, fontWeight: 500, lineHeight: '20px' },
  subtitle1: { fontFamily, fontSize: 14, fontWeight: 500, lineHeight: '20px' },
  body1: { fontFamily, fontSize: 13, fontWeight: 400, lineHeight: '18px' },
  body2: { fontFamily, fontSize: 11, fontWeight: 400, lineHeight: '16px' },
  caption: { fontFamily, fontSize: 11, fontWeight: 400, lineHeight: '16px' },
  overline: {
    fontFamily,
    fontSize: 10,
    fontWeight: 600,
    lineHeight: '16px',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
  button: { fontFamily, fontSize: 13, fontWeight: 500, textTransform: 'none' },
};
