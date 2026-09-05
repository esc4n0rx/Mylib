import i18n from '@/i18n';

export type ApiErrorKind =
  | 'network'
  | 'unauthorized'
  | 'forbidden'
  | 'notFound'
  | 'conflict'
  | 'validation'
  | 'rateLimit'
  | 'serverError'
  | 'unknown';

export class ApiError extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly kind: ApiErrorKind;
  readonly details: unknown;

  constructor(params: {
    status: number;
    code?: string;
    message: string;
    kind: ApiErrorKind;
    details?: unknown;
  }) {
    super(params.message);
    this.name = 'ApiError';
    this.status = params.status;
    this.code = params.code;
    this.kind = params.kind;
    this.details = params.details;
  }

  /** Localised, user-facing message. Prefers a known backend error code. */
  get localizedMessage(): string {
    const t = i18n.getFixedT(null, 'errors');
    if (this.code && i18n.exists(`errors:codes.${this.code}`)) {
      return t(`codes.${this.code}`);
    }
    return t(this.kind === 'unknown' ? 'generic' : this.kind);
  }
}

export function kindForStatus(status: number): ApiErrorKind {
  switch (status) {
    // The backend (AppError::validation) always answers bad input with 400, not 422 — keep
    // both mapped to 'validation' so an error code missing from errors:codes still degrades to
    // a sensible message instead of the generic fallback.
    case 400:
    case 422:
      return 'validation';
    case 401:
      return 'unauthorized';
    case 403:
      return 'forbidden';
    case 404:
      return 'notFound';
    case 409:
      return 'conflict';
    case 429:
      return 'rateLimit';
    default:
      return status >= 500 ? 'serverError' : 'unknown';
  }
}
