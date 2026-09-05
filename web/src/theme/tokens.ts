// Semantic design tokens for MyLib. Each mode has its own hand-tuned values so
// dark mode preserves hierarchy and contrast rather than being an inversion.

export interface SurfaceTokens {
  externalBackground: string;
  surface: string;
  surfaceContainerLowest: string;
  surfaceContainerLow: string;
  surfaceContainer: string;
  surfaceContainerHigh: string;
  surfaceContainerHighest: string;
  onSurface: string;
  onSurfaceVariant: string;
  primary: string;
  onPrimary: string;
  primaryContainer: string;
  onPrimaryContainer: string;
  sidebarActiveBg: string;
  sidebarActiveText: string;
  error: string;
  onError: string;
  errorContainer: string;
  onErrorContainer: string;
  outline: string;
  outlineVariant: string;
}

export const lightTokens: SurfaceTokens = {
  externalBackground: '#E9E8E5',
  surface: '#FBF9F8',
  surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#F6F3F2',
  surfaceContainer: '#F0EDED',
  surfaceContainerHigh: '#EAE8E7',
  surfaceContainerHighest: '#E4E2E1',
  onSurface: '#1B1C1C',
  onSurfaceVariant: '#404A3C',
  primary: '#0F6E11',
  onPrimary: '#FFFFFF',
  primaryContainer: '#75CE67',
  onPrimaryContainer: '#005706',
  sidebarActiveBg: '#75CE67',
  sidebarActiveText: '#163513',
  error: '#BA1A1A',
  onError: '#FFFFFF',
  errorContainer: '#FFDAD6',
  onErrorContainer: '#93000A',
  outline: '#707A6A',
  outlineVariant: '#BFCAB8',
};

export const darkTokens: SurfaceTokens = {
  externalBackground: '#111312',
  surface: '#171A18',
  surfaceContainerLowest: '#101210',
  surfaceContainerLow: '#1B1E1C',
  surfaceContainer: '#202320',
  surfaceContainerHigh: '#272A27',
  surfaceContainerHighest: '#2E312E',
  onSurface: '#E8EAE6',
  onSurfaceVariant: '#BBC5B5',
  primary: '#91E083',
  onPrimary: '#00390A',
  primaryContainer: '#245D25',
  onPrimaryContainer: '#C9F7BF',
  sidebarActiveBg: '#245D25',
  sidebarActiveText: '#C9F7BF',
  error: '#FFB4AB',
  onError: '#690005',
  errorContainer: '#93000A',
  onErrorContainer: '#FFDAD6',
  outline: '#8A9584',
  outlineVariant: '#3F493D',
};

// Extra MUI-inspired colour schemes. They intentionally share the same
// semantic token contract so every screen (including dialogs and the player)
// changes as a coherent theme instead of only swapping the primary colour.
export const oceanTokens: SurfaceTokens = {
  ...lightTokens,
  externalBackground: '#DDEAF0', surface: '#F7FAFC', surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#EDF5F8', surfaceContainer: '#E3EFF4', surfaceContainerHigh: '#D8E8EF',
  surfaceContainerHighest: '#CBDFE8', onSurface: '#102A36', onSurfaceVariant: '#405E69',
  primary: '#006780', onPrimary: '#FFFFFF', primaryContainer: '#A9EDFF', onPrimaryContainer: '#004D61',
  sidebarActiveBg: '#A9EDFF', sidebarActiveText: '#004D61', outline: '#6F8A94', outlineVariant: '#B8CDD5',
};

export const violetTokens: SurfaceTokens = {
  ...lightTokens,
  externalBackground: '#ECE7F2', surface: '#FCF8FF', surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#F5F0FA', surfaceContainer: '#EEE8F4', surfaceContainerHigh: '#E7E0EE',
  surfaceContainerHighest: '#DFD7E8', onSurface: '#261A2D', onSurfaceVariant: '#66566D',
  primary: '#77558D', onPrimary: '#FFFFFF', primaryContainer: '#F2DAFF', onPrimaryContainer: '#5E3D73',
  sidebarActiveBg: '#F2DAFF', sidebarActiveText: '#5E3D73', outline: '#8A798F', outlineVariant: '#D1C2D5',
};

export const sunsetTokens: SurfaceTokens = {
  ...lightTokens,
  externalBackground: '#F2E8DF', surface: '#FFF8F4', surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#FAF0EA', surfaceContainer: '#F4E8E0', surfaceContainerHigh: '#EEE0D6',
  surfaceContainerHighest: '#E7D7CB', onSurface: '#2C1D16', onSurfaceVariant: '#715548',
  primary: '#9A4522', onPrimary: '#FFFFFF', primaryContainer: '#FFDBCC', onPrimaryContainer: '#7B2E0D',
  sidebarActiveBg: '#FFDBCC', sidebarActiveText: '#7B2E0D', outline: '#957568', outlineVariant: '#D8C2B8',
};

export const roseTokens: SurfaceTokens = {
  ...lightTokens,
  externalBackground: '#F1E5E9', surface: '#FFF8F9', surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#FAEFF2', surfaceContainer: '#F4E6EA', surfaceContainerHigh: '#EEDEE3',
  surfaceContainerHighest: '#E7D5DB', onSurface: '#301A21', onSurfaceVariant: '#71545E',
  primary: '#9B405C', onPrimary: '#FFFFFF', primaryContainer: '#FFD9E2', onPrimaryContainer: '#7D2945',
  sidebarActiveBg: '#FFD9E2', sidebarActiveText: '#7D2945', outline: '#98717E', outlineVariant: '#DCC0C8',
};

export const midnightTokens: SurfaceTokens = {
  ...darkTokens,
  externalBackground: '#080F1D', surface: '#0E1726', surfaceContainerLowest: '#09111E',
  surfaceContainerLow: '#131E30', surfaceContainer: '#18253A', surfaceContainerHigh: '#1F2D44',
  surfaceContainerHighest: '#27364F', onSurface: '#E7EDF8', onSurfaceVariant: '#BAC6DA',
  primary: '#AFC6FF', onPrimary: '#102E60', primaryContainer: '#29477A', onPrimaryContainer: '#D9E2FF',
  sidebarActiveBg: '#29477A', sidebarActiveText: '#D9E2FF', outline: '#8491A8', outlineVariant: '#3E4B61',
};

export const radius = {
  appContainer: 22,
  card: 10,
  button: 8,
  input: 8,
  poster: 8,
  chip: 9999,
};

export const spacing = {
  stackDense: 4,
  stackCompact: 8,
  gridGutter: 16,
  appMargin: 24,
  sectionGap: 32,
};

export const appShadow = '0 8px 30px rgba(0,0,0,0.10)';
export const modalShadow = '0 12px 40px rgba(0,0,0,0.15)';
