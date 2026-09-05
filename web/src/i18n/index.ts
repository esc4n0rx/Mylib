import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import ptCommon from './locales/pt-BR/common.json';
import ptSetup from './locales/pt-BR/setup.json';
import ptAuth from './locales/pt-BR/auth.json';
import ptLibraries from './locales/pt-BR/libraries.json';
import ptErrors from './locales/pt-BR/errors.json';
import ptProfiles from './locales/pt-BR/profiles.json';
import ptRemoteSources from './locales/pt-BR/remoteSources.json';
import enCommon from './locales/en-US/common.json';
import enSetup from './locales/en-US/setup.json';
import enAuth from './locales/en-US/auth.json';
import enLibraries from './locales/en-US/libraries.json';
import enErrors from './locales/en-US/errors.json';
import enProfiles from './locales/en-US/profiles.json';
import enRemoteSources from './locales/en-US/remoteSources.json';

export const DEFAULT_LANGUAGE = 'pt-BR';
export const FALLBACK_LANGUAGE = 'pt-BR';

export const resources = {
  'pt-BR': {
    common: ptCommon,
    setup: ptSetup,
    auth: ptAuth,
    libraries: ptLibraries,
    errors: ptErrors,
    profiles: ptProfiles,
    remoteSources: ptRemoteSources,
  },
  'en-US': {
    common: enCommon,
    setup: enSetup,
    auth: enAuth,
    libraries: enLibraries,
    errors: enErrors,
    profiles: enProfiles,
    remoteSources: enRemoteSources,
  },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: DEFAULT_LANGUAGE,
  fallbackLng: FALLBACK_LANGUAGE,
  defaultNS: 'common',
  ns: ['common', 'setup', 'auth', 'libraries', 'errors', 'profiles', 'remoteSources'],
  interpolation: { escapeValue: false },
});

export default i18n;
