import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ThemeProvider } from '@mui/material';
import i18n from '@/i18n';
import { createAppTheme } from '@/theme/theme';
import type { M3uPreview } from '@/api';
import {
  M3uSelectionTree,
  emptySelection,
  selectionToRules,
} from '@/features/libraries/components/M3uSelectionTree';

const preview: M3uPreview = {
  totalEntries: 5,
  movieCandidates: 3,
  tvCandidates: 2,
  unknownCandidates: 0,
  categories: [
    {
      name: 'FILMES',
      mediaType: 'MOVIE',
      count: 3,
      subcategories: [
        { name: 'LANÇAMENTOS 2025', count: 2 },
        { name: 'AÇÃO', count: 1 },
      ],
    },
    { name: 'NETFLIX', mediaType: 'TV_SHOW', count: 2, subcategories: [] },
  ],
};

describe('remote sources selection', () => {
  it('converts UI selection state into backend rules, honouring spaces in names', () => {
    const state = emptySelection();
    state.all.add('MOVIE');
    state.subcategories.add(['TV_SHOW', 'NETFLIX', 'ORIGINAIS BRASIL'].join(String.fromCharCode(0)));

    const rules = selectionToRules(state);
    expect(rules).toContainEqual({
      mediaType: 'MOVIE',
      category: null,
      subcategory: null,
      includeAll: true,
      isEnabled: true,
    });
    expect(rules).toContainEqual({
      mediaType: 'TV_SHOW',
      category: 'NETFLIX',
      subcategory: 'ORIGINAIS BRASIL',
      includeAll: false,
      isEnabled: true,
    });
  });

  it('a selected whole media type supersedes its category rules', () => {
    const state = emptySelection();
    state.all.add('MOVIE');
    state.categories.add(['MOVIE', 'FILMES'].join(String.fromCharCode(0)));
    const rules = selectionToRules(state);
    expect(rules.filter((rule) => rule.mediaType === 'MOVIE')).toHaveLength(1);
    expect(rules[0]?.category).toBeNull();
  });

  it('renders the category tree in pt-BR', () => {
    render(
      <ThemeProvider theme={createAppTheme('light')}>
        <M3uSelectionTree preview={preview} value={emptySelection()} onChange={() => {}} />
      </ThemeProvider>,
    );
    expect(screen.getAllByText('Selecionar tudo').length).toBeGreaterThan(0);
    expect(screen.getByText('FILMES')).toBeInTheDocument();
    expect(screen.getByText('NETFLIX')).toBeInTheDocument();
  });

  it('exposes the Portuguese source copy', () => {
    expect(i18n.t('remoteSources:googleDrive.connect')).toBe('Conectar com Google');
    expect(i18n.t('remoteSources:form.analyze')).toBe('Analisar lista');
    expect(i18n.t('libraries:detail.sources')).toBe('Fontes');
  });
});
