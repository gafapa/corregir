import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nProvider, detectLanguage, useI18n } from "./i18n";

function Probe() {
  const { t, errorText, formatNumber, setLanguage, language } = useI18n();
  return <div>
    <p>{t("submissions.showing", { shown: 2, total: 5 })}</p>
    <p data-testid="error">{errorText(new Error("Error connecting to the inference server: connection refused"))}</p>
    <p data-testid="unknown">{errorText("unexpected technical detail")}</p>
    <p data-testid="number">{formatNumber(7.25)}</p>
    <button onClick={() => setLanguage(language === "gl" ? "es" : "gl")}>switch</button>
  </div>;
}

describe("interface language", () => {
  it("translates messages and known backend errors, keeping unknown errors visible", async () => {
    const user = userEvent.setup();
    render(<I18nProvider initialLanguage="gl"><Probe /></I18nProvider>);
    expect(screen.getByText("Amosando 2 de 5 entregas")).toBeInTheDocument();
    expect(screen.getByTestId("error")).toHaveTextContent("Non se pode conectar co servidor local de IA");
    expect(screen.getByTestId("unknown")).toHaveTextContent("unexpected technical detail");
    expect(screen.getByTestId("number")).toHaveTextContent("7,25");
    expect(document.documentElement.lang).toBe("gl");
    await user.click(screen.getByRole("button", { name: "switch" }));
    expect(screen.getByText("Mostrando 2 de 5 entregas")).toBeInTheDocument();
    expect(localStorage.getItem("corregir.language")).toBe("es");
    expect(detectLanguage()).toBe("es");
  });
  it("keeps English backend messages unchanged in English", () => {
    render(<I18nProvider initialLanguage="en"><Probe /></I18nProvider>);
    expect(screen.getByTestId("error")).toHaveTextContent("Error connecting to the inference server: connection refused");
  });
});
