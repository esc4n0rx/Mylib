import { describe, expect, it } from 'vitest';
import { libraryFormSchema, defaultLibraryValues } from '@/features/libraries/schema';

describe('libraryFormSchema', () => {
  it('accepts a valid public movie library', () => {
    const result = libraryFormSchema.safeParse({
      ...defaultLibraryValues,
      name: 'Filmes',
      paths: ['/mnt/media/movies'],
    });
    expect(result.success).toBe(true);
  });

  it('requires a matching password for private libraries', () => {
    const result = libraryFormSchema.safeParse({
      ...defaultLibraryValues,
      name: 'Privada',
      privacy: 'PRIVATE',
      password: 'secret1',
      confirmPassword: 'secret2',
      paths: ['/mnt/x'],
    });
    expect(result.success).toBe(false);
  });

  it('rejects an empty path list', () => {
    const result = libraryFormSchema.safeParse({
      ...defaultLibraryValues,
      name: 'Sem paths',
      paths: [],
    });
    expect(result.success).toBe(false);
  });
});
