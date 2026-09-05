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
import { LibrariesStep } from '../components/LibrariesStep';
import { FinishStep } from '../components/FinishStep';
import { ServerPreview } from '../components/ServerPreview';

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
      await api.setup.submit({
        serverName: store.server.serverName.trim(),
        database: store.database,
        administrator: {
          username: store.admin.username,
          password: store.admin.password,
          displayName: store.admin.displayName,
        },
      });

      // The wizard stays visually continuous: authenticate, then create the
      // libraries the user queued during onboarding.
      const loginResponse = await api.auth.login(store.admin.username, store.admin.password);
      login(loginResponse.accessToken);

      for (const library of store.libraries) {
        await api.libraries.create(library);
      }

      await queryClient.invalidateQueries({ queryKey: ['setup', 'status'] });
      notify(t('finish.title'), 'success');
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
        step === 0 ? (
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
            disabled={step === 0 || submitting}
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
              {step === 3 && store.libraries.length === 0
                ? t('libraries.skip')
                : tc('actions.continue')}
            </Button>
          )}
        </>
      }
    >
      {step === 0 ? (
        <ServerStep
          formId={formId}
          values={store.server}
          onChange={store.setServer}
          onNext={(values) => {
            store.setServer(values);
            store.setStep(1);
          }}
        />
      ) : null}
      {step === 1 ? (
        <AdminStep
          formId={formId}
          values={store.admin}
          onNext={(values) => {
            store.setAdmin(values);
            store.setStep(2);
          }}
        />
      ) : null}
      {step === 2 ? (
        <DatabaseStep
          formId={formId}
          value={store.database}
          onNext={(value) => {
            store.setDatabase(value);
            store.setStep(3);
          }}
        />
      ) : null}
      {step === 3 ? (
        <LibrariesStep
          formId={formId}
          libraries={store.libraries}
          onAdd={store.addLibrary}
          onRemove={store.removeLibrary}
          onNext={() => store.setStep(4)}
        />
      ) : null}
      {step === 4 ? (
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
