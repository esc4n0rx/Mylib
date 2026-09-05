import { useState } from 'react';
import { Button } from '@mui/material';
import ArrowForwardIcon from '@mui/icons-material/ArrowForward';
import { useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, ApiError } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { useToast } from '@/app/ToastProvider';
import { SetupShell } from '@/layouts/SetupShell';
import { useSetupStore } from '../store';
import { ServerStep } from '../components/ServerStep';
import { AdminStep } from '../components/AdminStep';
import { DatabaseStep } from '../components/DatabaseStep';
import { MetadataStep } from '../components/MetadataStep';
import { LibrariesStep } from '../components/LibrariesStep';
import { FinishStep } from '../components/FinishStep';
import { ServerPreview } from '../components/ServerPreview';

const STEP_SERVER = 0;
const STEP_ADMIN = 1;
const STEP_DATABASE = 2;
const STEP_METADATA = 3;
const STEP_LIBRARIES = 4;
const STEP_FINISH = 5;

export default function SetupPage() {
  const { t } = useTranslation('setup');
  const { t: tc } = useTranslation('common');
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { login } = useAuth();
  const { notify } = useToast();

  const store = useSetupStore();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const steps = [
    t('steps.server'),
    t('steps.administrator'),
    t('steps.database'),
    t('steps.metadata'),
    t('steps.libraries'),
    t('steps.finish'),
  ];
  const step = store.step;
  const formId = `setup-step-${step}`;
  const isLast = step === steps.length - 1;

  const finalize = async () => {
    if (!store.admin) return;
    setSubmitting(true);
    setError(null);
    try {
      // The account-creation call and the two follow-up calls below (login, then create each
      // queued library) are NOT one atomic operation on the server: by the time this call
      // returns successfully, the admin account already exists and setup is marked complete.
      // If a later step in this function fails, we must not leave the user stuck on this
      // screen with no way forward — a retry here would otherwise hit SETUP_ALREADY_COMPLETED
      // forever, since the account was already created on the first attempt.
      try {
        await api.setup.submit({
          serverName: store.server.serverName.trim(),
          database: store.database,
          administrator: {
            username: store.admin.username,
            password: store.admin.password,
            displayName: store.admin.displayName,
          },
          tmdbApiKey: store.tmdbApiKey.trim() || undefined,
        });
      } catch (err) {
        const alreadyCompleted =
          err instanceof ApiError && err.code === 'SETUP_ALREADY_COMPLETED';
        if (!alreadyCompleted) throw err;
        // A previous attempt already created the account (a later step failed back then) —
        // fall through to login instead of getting stuck here.
      }

      const loginResponse = await api.auth.login(store.admin.username, store.admin.password);
      login(loginResponse.accessToken);

      // Library creation can fail per-item (e.g. a path that doesn't exist yet on this
      // machine). That should not undo the setup that already succeeded above — skip the
      // failed ones and let the user know, instead of leaving the whole wizard stuck.
      const failedLibraries: string[] = [];
      for (const library of store.libraries) {
        try {
          await api.libraries.create(library);
        } catch {
          failedLibraries.push(library.name);
        }
      }

      await queryClient.invalidateQueries({ queryKey: ['setup', 'status'] });
      if (failedLibraries.length > 0) {
        notify(t('finish.librariesFailed', { names: failedLibraries.join(', ') }), 'warning');
      } else {
        notify(t('finish.title'), 'success');
      }
      store.reset();
      navigate('/home', { replace: true });
    } catch (err) {
      setError(err instanceof ApiError ? err.localizedMessage : tc('states.errorBody'));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <SetupShell
      activeStep={step}
      steps={steps}
      aside={
        step === STEP_SERVER ? (
          <ServerPreview
            name={store.server.serverName}
            language={store.server.serverLanguage}
          />
        ) : undefined
      }
      footer={
        <>
          <Button
            onClick={() => store.setStep(Math.max(0, step - 1))}
            disabled={step === STEP_SERVER || submitting}
          >
            {tc('actions.back')}
          </Button>
          {isLast ? (
            <Button variant="contained" onClick={finalize} disabled={submitting}>
              {submitting ? t('finish.submitting') : t('finish.cta')}
            </Button>
          ) : (
            <Button
              type="submit"
              form={formId}
              variant="contained"
              endIcon={<ArrowForwardIcon />}
            >
              {step === STEP_METADATA && !store.tmdbApiKey.trim()
                ? t('metadata.skip')
                : step === STEP_LIBRARIES && store.libraries.length === 0
                  ? t('libraries.skip')
                  : tc('actions.continue')}
            </Button>
          )}
        </>
      }
    >
      {step === STEP_SERVER ? (
        <ServerStep
          formId={formId}
          values={store.server}
          onChange={store.setServer}
          onNext={(values) => {
            store.setServer(values);
            store.setStep(STEP_ADMIN);
          }}
        />
      ) : null}
      {step === STEP_ADMIN ? (
        <AdminStep
          formId={formId}
          values={store.admin}
          onNext={(values) => {
            store.setAdmin(values);
            store.setStep(STEP_DATABASE);
          }}
        />
      ) : null}
      {step === STEP_DATABASE ? (
        <DatabaseStep
          formId={formId}
          value={store.database}
          onNext={(value) => {
            store.setDatabase(value);
            store.setStep(STEP_METADATA);
          }}
        />
      ) : null}
      {step === STEP_METADATA ? (
        <MetadataStep
          formId={formId}
          value={store.tmdbApiKey}
          onChange={store.setTmdbApiKey}
          onNext={() => store.setStep(STEP_LIBRARIES)}
        />
      ) : null}
      {step === STEP_LIBRARIES ? (
        <LibrariesStep
          formId={formId}
          libraries={store.libraries}
          onAdd={store.addLibrary}
          onRemove={store.removeLibrary}
          onNext={() => store.setStep(STEP_FINISH)}
        />
      ) : null}
      {step === STEP_FINISH ? (
        <FinishStep
          serverName={store.server.serverName}
          database={store.database}
          adminUsername={store.admin?.username ?? ''}
          libraries={store.libraries}
          error={error}
        />
      ) : null}
    </SetupShell>
  );
}
