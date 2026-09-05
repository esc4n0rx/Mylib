import { z } from 'zod';

export const LANGUAGE_OPTIONS = [
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'en-US', label: 'English (US)' },
  { value: 'es-ES', label: 'Español' },
  { value: 'fr-FR', label: 'Français' },
  { value: 'de-DE', label: 'Deutsch' },
  { value: 'ja-JP', label: '日本語' },
] as const;

export const REGION_OPTIONS = [
  { value: 'BR', label: 'Brasil' },
  { value: 'US', label: 'United States' },
  { value: 'GB', label: 'United Kingdom' },
  { value: 'ES', label: 'España' },
  { value: 'FR', label: 'France' },
] as const;

export const libraryFormSchema = z
  .object({
    name: z.string().trim().min(1).max(128),
    description: z.string().trim().max(500).optional().or(z.literal('')),
    type: z.enum(['MOVIE', 'TV_SHOW']),
    privacy: z.enum(['PUBLIC', 'PRIVATE']),
    password: z.string().optional().or(z.literal('')),
    confirmPassword: z.string().optional().or(z.literal('')),
    minimumAge: z.coerce.number().int().min(0).max(21),
    metadataLanguage: z.string().min(2),
    metadataRegion: z.string().optional().or(z.literal('')),
    paths: z.array(z.string().trim().min(1)).min(1),
  })
  .superRefine((value, ctx) => {
    if (value.privacy === 'PRIVATE') {
      if (!value.password || value.password.length < 4) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['password'],
          message: 'required',
        });
      }
      if (value.password !== value.confirmPassword) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['confirmPassword'],
          message: 'mismatch',
        });
      }
    }
  });

export type LibraryFormValues = z.infer<typeof libraryFormSchema>;

export const defaultLibraryValues: LibraryFormValues = {
  name: '',
  description: '',
  type: 'MOVIE',
  privacy: 'PUBLIC',
  password: '',
  confirmPassword: '',
  minimumAge: 0,
  metadataLanguage: 'pt-BR',
  metadataRegion: 'BR',
  paths: [],
};

export function toCreateRequest(values: LibraryFormValues) {
  return {
    name: values.name.trim(),
    description: values.description?.trim() || undefined,
    type: values.type,
    privacy: values.privacy,
    password: values.privacy === 'PRIVATE' ? values.password : undefined,
    minimumAge: values.minimumAge,
    metadataLanguage: values.metadataLanguage,
    metadataRegion: values.metadataRegion || undefined,
    paths: values.paths,
  };
}
