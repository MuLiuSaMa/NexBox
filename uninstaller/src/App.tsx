import { useState, useCallback } from "react";
import TitleBar from "./components/TitleBar";
import UninstallerLayout from "./components/UninstallerLayout";
import ConfirmPage from "./pages/ConfirmPage";
import UninstallingPage from "./pages/UninstallingPage";
import CompletePage from "./pages/CompletePage";

export default function App() {
  const [step, setStep] = useState(1);
  const [error, setError] = useState("");

  const handleStartUninstall = useCallback(() => {
    setStep(2);
  }, []);

  const handleUninstallComplete = useCallback(() => {
    setStep(3);
  }, []);

  const handleUninstallError = useCallback((msg: string) => {
    setError(msg);
    alert(msg);
  }, []);

  return (
    <>
      <TitleBar />
      <UninstallerLayout>
        {step === 1 && <ConfirmPage onStart={handleStartUninstall} />}
        {step === 2 && (
          <UninstallingPage
            onComplete={handleUninstallComplete}
            onError={handleUninstallError}
          />
        )}
        {step === 3 && <CompletePage />}
      </UninstallerLayout>
    </>
  );
}
