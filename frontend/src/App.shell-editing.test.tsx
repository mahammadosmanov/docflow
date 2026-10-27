import { describe, expect, it } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { App } from "./App";
import { interceptMockBridge, signIn, registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: preferences, editor and shortcuts", () => {
  it("opens preferences before authentication", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "DocFlow" });
    fireEvent.click(screen.getByRole("button", { name: "Ayarlar" }));
    expect(
      await screen.findByRole("heading", { name: "Ayarlar" }),
    ).toBeVisible();
    expect(screen.getByText("Dil")).toBeVisible();
    expect(screen.getByText("Tema")).toBeVisible();
    expect(
      screen.getByText("Windows açılışında DocFlow'u başlat"),
    ).toBeVisible();
    expect(screen.getByText("Onaysız RFI Sistemi eşitleme aktif")).toBeVisible();
  });

  it("previews and explicitly copies a local redacted support bundle without sending it", async () => {
    const content = `${JSON.stringify(
      {
        schemaVersion: 1,
        docflow: { version: "0.6.0" },
        platform: { arch: "aarch64", os: "macos" },
        components: {
          ocr: {
            engine: "tesseract",
            languageDataAvailable: true,
            status: "available",
          },
          sidecar: { required: false, status: "retired" },
        },
        recentErrorCodes: ["validation"],
        privacy: { delivery: "local-only", redaction: "recursive" },
      },
      null,
      2,
    )}\n`;
    const call = interceptMockBridge((method) =>
      method === "support/diagnostics"
        ? {
            handled: true,
            value: {
              fileName: "docflow-support-diagnostics-v1.json",
              mediaType: "application/json",
              byteLength: content.length,
              content,
            },
          }
        : method === "support/copyDiagnostics"
          ? { handled: true, value: { ok: true } }
          : { handled: false },
    );

    render(<App />);
    await screen.findByRole("heading", { name: "DocFlow" });
    fireEvent.click(screen.getByRole("button", { name: "Ayarlar" }));
    await screen.findByRole("heading", { name: "Ayarlar" });

    expect(screen.getByText(/hiçbir yere gönderilmez/i)).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: "Destek paketini önizle" }),
    );

    const preview = await screen.findByLabelText("Destek paketi önizlemesi");
    expect(preview).toHaveTextContent('"version": "0.6.0"');
    expect(preview).toHaveTextContent('"status": "retired"');
    expect(preview).toHaveTextContent('"recentErrorCodes"');
    expect(
      call.mock.calls.filter(([method]) => method === "support/diagnostics"),
    ).toHaveLength(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Paketi panoya kopyala" }),
    );
    await screen.findByText("Destek paketi panoya kopyalandı.");
    expect(call).toHaveBeenCalledWith("support/copyDiagnostics", { content });
    expect(
      call.mock.calls.some(([method]) => /upload|send|telemetry/i.test(method)),
    ).toBe(false);
  });

  it("starts print clean and restores only previous safe settings on request", async () => {
    const savedParams: Record<string, unknown>[] = [];
    interceptMockBridge((method, params) => {
      if (method === "settings/params") {
        return {
          handled: true,
          value: {
            files: ["C:/stale/old-job.pdf"],
            paper: "A3",
            copies: 9,
            duplex: "long",
          },
        };
      }
      if (method === "settings/saveParams") {
        savedParams.push(
          (params as { values: Record<string, unknown> }).values,
        );
        return { handled: true, value: { ok: true } };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );

    const copiesInput = await screen.findByRole("spinbutton", {
      name: /^Kopya sayısı/,
    });
    expect(copiesInput).toHaveValue(1);
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A4");
    expect(screen.queryByText("old-job.pdf")).toBeNull();

    fireEvent.click(
      await screen.findByRole("button", { name: "Önceki ayarları getir" }),
    );
    expect(copiesInput).toHaveValue(9);
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A3");
    expect(screen.getByRole("combobox", { name: /^Çift taraflı/ })).toHaveValue(
      "long",
    );

    fireEvent.change(copiesInput, { target: { value: "7" } });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 250));
    });
    expect(savedParams).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    await waitFor(() => {
      expect(savedParams).toHaveLength(1);
    });
    expect(savedParams[0]).toMatchObject({ copies: 7, paper: "A3" });
    expect(savedParams[0]).not.toHaveProperty("files");
  });

  it("filters and searches rows in the preview editor table", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: /^Gözat/ }));
    await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol edin ve düzenleyin"),
    ).toBeVisible();

    const searchInput = screen.getByLabelText(
      "Ara (RFI no, durum, açıklama)...",
    );
    expect(searchInput).toBeVisible();

    // Search for 0008
    fireEvent.change(searchInput, { target: { value: "0008" } });
    expect(screen.getByRole("button", { name: "0008" })).toBeVisible();

    // Filter by changes tab
    const changesTab = screen.getByRole("button", { name: "Değişiklikler" });
    fireEvent.click(changesTab);
    expect(changesTab).toHaveClass("filter-tab--active");

    // Clear search
    fireEvent.change(searchInput, { target: { value: "" } });
    const allTab = screen.getByRole("button", { name: "Tümü" });
    fireEvent.click(allTab);
    expect(allTab).toHaveClass("filter-tab--active");
  });

  it("supports keyboard shortcuts Ctrl+Enter and Escape", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: /^Gözat/ }));
    await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx");

    // Press Ctrl+Enter to trigger preview
    fireEvent.keyDown(window, { key: "Enter", ctrlKey: true });

    expect(
      await screen.findByText(
        "RFI kayıtlarını kontrol edin ve düzenleyin",
        undefined,
        { timeout: 10000 },
      ),
    ).toBeVisible();

    // Press Escape to return to form
    fireEvent.keyDown(window, { key: "Escape" });

    expect(
      await screen.findByRole("button", { name: "Önizle" }, { timeout: 10000 }),
    ).toBeVisible();
  });

  it("supports quick theme toggling via header button and Alt+T shortcut", async () => {
    let currentTheme = "system";
    interceptMockBridge((method, params) => {
      if (method === "settings/update") {
        currentTheme = (params.theme as string) || currentTheme;
        return {
          handled: true,
          value: {
            locale: null,
            theme: currentTheme,
            font: "system",
            fontScale: 1.0,
            compact: false,
          },
        };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();

    const toggleBtn = screen.getByRole("button", {
      name: /Temayı değiştir/i,
    });
    expect(toggleBtn).toBeVisible();

    // Initial is system
    expect(document.documentElement.dataset.theme).toBe("system");

    // Click toggle: system -> light
    fireEvent.click(toggleBtn);
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("light");
    });

    // Press Alt+T: light -> dark
    fireEvent.keyDown(window, { key: "t", altKey: true });
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("dark");
    });
  });

  it("shows quick clear button for browse path fields and clears on click", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    const browseBtn = await screen.findByRole("button", { name: /^Gözat/ });
    fireEvent.click(browseBtn);

    // Verify clear button appears
    const clearBtn = await screen.findByRole("button", { name: "Temizle" });
    expect(clearBtn).toBeVisible();

    // Click clear
    fireEvent.click(clearBtn);

    // Verify clear button disappears and placeholder returns
    await waitFor(() => {
      expect(
        screen.queryByRole("button", { name: "Temizle" }),
      ).not.toBeInTheDocument();
    });
  });
});
