import { z } from 'zod';

export const serverStepSchema = z.object({
  serverName: z.string().trim().min(1).max(128),
  serverLanguage: z.string().min(2),
});

export const adminStepSchema = z
  .object({
    username: z
      .string()
      .trim()
      .min(3)
      .max(32)
      .regex(/^[a-zA-Z0-9_.-]+$/),
    displayName: z.string().trim().min(1).max(64),
    email: z.string().trim().email().optional().or(z.literal('')),
    // Must match the backend's validate_password minimum (see src/features/auth/mod.rs) —
    // otherwise the wizard accepts a password here that setup then rejects with a generic,
    // unhelpful error on the last step.
    password: z.string().min(10).max(128),
    confirmPassword: z.string(),
  })
  .refine((v) => v.password === v.confirmPassword, {
    path: ['confirmPassword'],
    message: 'mismatch',
  });

export const mysqlSchema = z.object({
  host: z.string().trim().min(1),
  port: z.coerce.number().int().min(1).max(65535),
  database: z.string().trim().min(1),
  username: z.string().trim().min(1),
  password: z.string().min(1),
  sslMode: z.enum(['disabled', 'preferred', 'required']),
});

export type ServerStepValues = z.infer<typeof serverStepSchema>;
export type AdminStepValues = z.infer<typeof adminStepSchema>;
export type MysqlValues = z.infer<typeof mysqlSchema>;

export const SERVER_LANGUAGE_OPTIONS = [
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'en-US', label: 'English (US)' },
  { value: 'es-ES', label: 'Español' },
  { value: 'fr-FR', label: 'Français' },
];
