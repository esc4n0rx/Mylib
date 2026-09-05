---
name: Executive Media Management
colors:
  surface: '#fbf9f8'
  surface-dim: '#dcd9d9'
  surface-bright: '#fbf9f8'
  surface-container-lowest: '#ffffff'
  surface-container-low: '#f6f3f2'
  surface-container: '#f0eded'
  surface-container-high: '#eae8e7'
  surface-container-highest: '#e4e2e1'
  on-surface: '#1b1c1c'
  on-surface-variant: '#404a3c'
  inverse-surface: '#303030'
  inverse-on-surface: '#f3f0f0'
  outline: '#707a6a'
  outline-variant: '#bfcab8'
  surface-tint: '#0f6e11'
  primary: '#0f6e11'
  on-primary: '#ffffff'
  primary-container: '#75ce67'
  on-primary-container: '#005706'
  inverse-primary: '#81db72'
  secondary: '#46673f'
  on-secondary: '#ffffff'
  secondary-container: '#c4eab8'
  on-secondary-container: '#4a6b43'
  tertiary: '#934750'
  on-tertiary: '#ffffff'
  tertiary-container: '#ff9fa8'
  on-tertiary-container: '#7a333c'
  error: '#ba1a1a'
  on-error: '#ffffff'
  error-container: '#ffdad6'
  on-error-container: '#93000a'
  primary-fixed: '#9df88b'
  primary-fixed-dim: '#81db72'
  on-primary-fixed: '#002201'
  on-primary-fixed-variant: '#005305'
  secondary-fixed: '#c7edbb'
  secondary-fixed-dim: '#abd1a0'
  on-secondary-fixed: '#032103'
  on-secondary-fixed-variant: '#2e4e29'
  tertiary-fixed: '#ffdadb'
  tertiary-fixed-dim: '#ffb2b8'
  on-tertiary-fixed: '#3d0410'
  on-tertiary-fixed-variant: '#763039'
  background: '#fbf9f8'
  on-background: '#1b1c1c'
  surface-variant: '#e4e2e1'
typography:
  page-title:
    fontFamily: Geist
    fontSize: 26px
    fontWeight: '600'
    lineHeight: 32px
    letterSpacing: -0.02em
  section-title:
    fontFamily: Geist
    fontSize: 16px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: -0.01em
  card-title:
    fontFamily: Geist
    fontSize: 14px
    fontWeight: '500'
    lineHeight: 20px
  body-md:
    fontFamily: Geist
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
  caption:
    fontFamily: Geist
    fontSize: 11px
    fontWeight: '400'
    lineHeight: 16px
  label-caps:
    fontFamily: Geist
    fontSize: 10px
    fontWeight: '600'
    lineHeight: 12px
    letterSpacing: 0.05em
rounded:
  sm: 0.25rem
  DEFAULT: 0.5rem
  md: 0.75rem
  lg: 1rem
  xl: 1.5rem
  full: 9999px
spacing:
  app-container-margin: 24px
  section-gap: 32px
  grid-gutter: 16px
  stack-compact: 8px
  stack-dense: 4px
---

## Brand & Style

The design system is centered on a **Minimalist, Professional, and High-Density** aesthetic tailored for high-end media server management. It prioritizes information hierarchy and utility, ensuring that vast media libraries remain navigable and visually serene.

The style leans into **Modern Corporate** principles—utilizing generous whitespace within an enclosed application surface to create a "contained" desktop-app feel. The emotional response is one of control, precision, and sophistication. By using a muted, off-white environment and a vibrant green accent, the interface feels both clinical and alive.

## Colors

The palette is built on a foundation of sophisticated neutrals. The external background (`#E9E8E5`) acts as a frame for the main application surface, which uses a slightly warmer, very light grey (`#FAFAF8`).

**Primary Accent:** The "MyLib Green" (`#75CE67`) is the sole driver of action. It is used for active states, primary buttons, and progress indicators.
**Typography:** Text is never pure black. Graphite (`#343434`) provides high contrast without the harshness of `#000`, while secondary and tertiary greys handle metadata and descriptions to maintain visual density without clutter.

## Typography

This design system utilizes **Geist** for its technical precision and exceptional legibility at small sizes, which is critical for a high-density management platform.

- **Scale:** The scale is intentionally tight. The 13px body size allows for more data on screen without sacrificing readability.
- **Hierarchy:** Use `page-title` for main dashboard headers. `section-title` should be used for group headings within a page.
- **Metadata:** All secondary media info (year, resolution, codec) should utilize the `caption` style in `text_tertiary`.

## Layout & Spacing

The layout follows a **Fixed-Fluid Hybrid** model. The main application is housed within a rounded container with a 24px margin from the viewport edges, creating a "floating" effect over the external background.

- **Grid:** Use a 12-column grid for dashboard layouts. Media galleries should use a responsive auto-fit grid with a minimum width of 140px for posters (2:3) and 280px for thumbnails (16:9).
- **Density:** Spacing is tight to reflect a professional tool. Use `stack-compact` for related items (label + input) and `stack-dense` for metadata clusters.
- **Breakpoints:** On tablet, the app container margin reduces to 12px. On mobile, the external background is hidden, and the application surface fills the screen entirely.

## Elevation & Depth

This design system uses a **Tonal Layering** approach combined with a singular, high-quality ambient shadow for the main application container.

- **App Container:** Uses the `Main Shadow` (0 8px 30px rgba(0,0,0,0.10)) to lift the entire workspace off the `#E9E8E5` base.
- **Cards & Surfaces:** Cards sit flat on the application surface with a `border_subtle` (1px solid) rather than individual shadows. This maintains a clean, professional "SaaS" look.
- **Modals:** Use a higher elevation with a more focused shadow (0 12px 40px rgba(0,0,0,0.15)) and a backdrop blur of 8px over the application surface.

## Shapes

The shape language is sophisticated, moving from high-radius outer containers to more precise, lower-radius inner elements.

- **Containers:** The main application wrapper uses a large `22px` radius.
- **Media:** Posters and thumbnails use an `8px` radius to maximize content visibility while softening the grid.
- **Interactive:** Buttons and Inputs share an `8px` radius, providing a sturdy, professional feel.
- **Filters:** Chips use a fully pill-shaped (circular ends) radius to distinguish them from actionable buttons.

## Components

### Buttons & Inputs
- **Primary Button:** Height 38px. Background: `#75CE67`, Text: `#163513` (High-contrast dark green).
- **Ghost Button:** Height 38px. Border: `border_subtle`, Text: `text_primary`.
- **Inputs:** Height 40px. Background: `#FFFFFF`, Border: `border_subtle`. On focus, border color changes to `primary_color_hex` with a 2px outer glow.

### Navigation & Sidebar
- **Sidebar Items:** Subtle hover state using `background_secondary`.
- **Active State:** Background `#7AD46D` (slightly lighter than primary), Text and Icon: `#163513`.
- **Width:** Sidebar should be fixed at 240px.

### Media Cards
- **Poster (2:3):** 1px subtle inner border to define edges on dark content. Titles placed below the image using `card-title`.
- **Thumbnail (16:9):** Used for "Continue Watching" or "Live TV". Includes a bottom-aligned 4px progress bar using `primary_color_hex`.

### Chips & Filters
- **Pill Filters:** Background: `#FFFFFF`, Border: `border_subtle`. When active: Background: `#343434`, Text: `#FFFFFF`.

### Dividers
- Use 1px solid `border_subtle`. Avoid using dividers where whitespace can suffice to maintain the minimalist aesthetic.