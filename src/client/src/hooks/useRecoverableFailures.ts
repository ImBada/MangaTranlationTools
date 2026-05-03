import React from "react";

export type RecoverableFailureId =
  | "analysis-run"
  | "analysis-sync"
  | "chapter-save"
  | "inpaint-mask-save"
  | "inpaint-result-save"
  | "inpaint-run";

export type RecoverableFailure = {
  id: RecoverableFailureId;
  message: string;
  title: string;
};

type RecoverableFailureInput = {
  id: RecoverableFailureId;
  message: string;
  title: string;
};

type UseRecoverableFailuresState = {
  clearRecoverableFailure: (id: RecoverableFailureId) => void;
  recoverableFailures: RecoverableFailure[];
  reportRecoverableFailure: (failure: RecoverableFailureInput) => void;
};

export function useRecoverableFailures(): UseRecoverableFailuresState {
  const [recoverableFailures, setRecoverableFailures] = React.useState<RecoverableFailure[]>([]);

  const clearRecoverableFailure = React.useCallback((id: RecoverableFailureId) => {
    setRecoverableFailures((failures) => failures.filter((failure) => failure.id !== id));
  }, []);

  const reportRecoverableFailure = React.useCallback((failure: RecoverableFailureInput) => {
    setRecoverableFailures((failures) => {
      const next = { ...failure };
      return [next, ...failures.filter((current) => current.id !== failure.id)];
    });
  }, []);

  return {
    clearRecoverableFailure,
    recoverableFailures,
    reportRecoverableFailure
  };
}
