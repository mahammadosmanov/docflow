import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { App } from "./App";
import { resetBridgeForTests } from "./lib/bridge";

async function signIn() {
  expect(
    await screen.findByRole("heading", { name: "DocFlow" }),
  ).toBeVisible();
  fireEvent.change(
    screen.getByLabelText("E-posta"),
    { target: { value: "demo@example.com" } },
  );
  fireEvent.change(screen.getByLabelText("Parola"), {
    target: { value: "secret1" },
  });
  const submit = document.querySelector<HTMLButtonElement>(
    'button[type="submit"]',
  );
  expect(submit).not.toBeNull();
  fireEvent.click(submit!);
  await screen.findByRole("heading", { name: "Çalışma alanı" });
}

afterEach(() => {
  cleanup();
  resetBridgeForTests();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-font");
  document.documentElement.removeAttribute("data-compact");
});

describe("DocFlow shell", () => {
  it("requires account email and explains that username sign-in is unavailable", async () => {
    render(<App />);

    await screen.findByRole("heading", { name: "DocFlow" });
    expect(screen.getByLabelText("E-posta")).toBeVisible();
    expect(
      screen.getByText(
        "Kullanıcı adıyla giriş kullanılamıyor. Hesabının e-posta adresini kullan.",
      ),
    ).toBeVisible();
  });

  it("shows the exact runtime product version in the workspace", async () => {
    render(<App />);
    await signIn();

    expect(screen.getByText("v0.6.0-demo")).toBeVisible();
  });

  it("signs in and opens the schema-driven operation", async () => {
    render(<App />);
    await signIn();

    expect(
      await screen.findByRole("heading", { name: "İşlemler" }),
    ).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: /Taramaları İşle/ }),
    );

    expect(
      await screen.findByRole("heading", { name: "Ayarlar" }),
    ).toBeVisible();
    expect(screen.getByText("Kaynak klasör")).toBeVisible();
    expect(screen.getByText("Hedef klasör")).toBeVisible();
  });

  it("exposes every preserved feature from the workspace", async () => {
    render(<App />);
    await signIn();

    expect(screen.getByText("Diğer özellikler")).toBeVisible();
    expect(
      screen.getByText("Bekleyen dosyaları tek tek kontrol et ve yerleştir."),
    ).toBeVisible();
    expect(
      screen.getByText("Yapılan işlemleri gör ve güvenle geri al."),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Dil, görünüm, yazı ve hesap tercihlerini yönet.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Kullanıcı erişimlerini ve yönetici yetkilerini yönet.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Bekleyen günlükleri ve belirsiz yazdırma işlerini güvenle incele.",
      ),
    ).toBeVisible();

    fireEvent.click(
      screen
        .getByText("Yapılan işlemleri gör ve güvenle geri al.")
        .closest("button")!,
    );
    const historyHeadings = await screen.findAllByRole("heading", {
      name: "Geçmiş",
    });
    expect(historyHeadings[0]).toBeVisible();
  });

  it("opens a recovery center for pending journals and uncertain print queues", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen
        .getByText(
          "Bekleyen günlükleri ve belirsiz yazdırma işlerini güvenle incele.",
        )
        .closest("button")!,
    );

    expect(
      (await screen.findAllByRole("heading", { name: "Kurtarma merkezi" }))[0],
    ).toBeVisible();
    expect(
      await screen.findByText("Sonuç bilinmiyor — otomatik tekrar yok"),
    ).toBeVisible();
    expect(await screen.findByText("Belirsiz yazdırma kuyruğu")).toBeVisible();
  });

  it("never offers undo for cloud history without a local journal", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen
        .getByText("Yapılan işlemleri gör ve güvenle geri al.")
        .closest("button")!,
    );

    await screen.findAllByRole("heading", { name: "Geçmiş" });
    expect(
      await screen.findAllByRole("button", { name: "Geri al" }),
    ).toHaveLength(1);
    expect(
      screen.getByText("Geri almak için bu cihazdaki yerel günlük gerekli."),
    ).toBeVisible();
  });

  it("renders the restored v0.5.4 RFI and print operation forms", async () => {
    render(<App />);
    await signIn();

    expect(
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", {
        name: /İmzalı RFI ilk sayfalarını hazırla/,
      }),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    );
    expect(await screen.findByText("RFI Log Excel dosyası")).toBeVisible();
    expect(screen.getByText("Sheet'ler ve RFI adetleri")).toBeVisible();
    expect(screen.getByText("Talep mailini buraya yapıştır")).toBeVisible();
    expect(
      screen.queryByText("Gemini API anahtarı (yalnız Gemini için)"),
    ).toBeNull();

    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "gemini" },
    });
    expect(
      screen.getByText("Gemini API anahtarı (yalnız Gemini için)"),
    ).toBeVisible();
  });

  it("renders the signed first-page staging form from its schema", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", {
        name: /İmzalı RFI ilk sayfalarını hazırla/,
      }),
    );
    expect(await screen.findByText("Yeni imzalı taramalar klasörü")).toBeVisible();
    expect(screen.getByText("Mevcut RFI arşivinin kökü")).toBeVisible();
    expect(screen.getByText("Review çıktı klasörü")).toBeVisible();
    expect(
      screen.getByText("Yeni taramaların alt klasörlerini de tara"),
    ).toBeVisible();
  });

  it("bulk-edits RFI rows and keeps automatic fields behind a toggle", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "0008" })).toHaveAttribute(
      "title",
      "AZE-SUN-INT-QMS-RFI-CIV-0008",
    );
    const itpSelects = screen.getAllByLabelText("ITP");
    expect(itpSelects[0]).toHaveValue("AZE-SUN-INT-QMS-ITP-CIV-0005");
    expect(
      within(itpSelects[0]).getByRole("option", { name: "5" }),
    ).toBeVisible();
    const itemSelects = screen.getAllByLabelText("Item");
    expect(itemSelects[0]).toHaveValue("CIV-5.2.1");
    expect(
      within(itemSelects[0]).getByRole("option", { name: "5.2.1" }),
    ).toBeVisible();

    const issueDays = screen.getAllByLabelText("Veriliş — Gün");
    const issueMonths = screen.getAllByLabelText("Veriliş — Ay");
    expect(issueDays[0]).toHaveValue("15");
    expect(issueMonths[0]).toHaveValue("8");
    expect(issueDays[0].closest(".compact-date-cell")).toHaveAttribute(
      "title",
      "15-Aug-26",
    );
    expect(screen.queryByLabelText("Veriliş — Yıl")).toBeNull();
    fireEvent.click(screen.getAllByLabelText("Veriliş — Yılı değiştir")[0]);
    expect(screen.getByLabelText("Veriliş — Yıl")).toHaveValue("2026");

    fireEvent.change(screen.getByLabelText("Değiştirilecek alan"), {
      target: { value: "dateOfIssue" },
    });
    const bulkIssueDay = screen.getAllByLabelText("Veriliş — Gün")[0];
    const bulkIssueMonth = screen.getAllByLabelText("Veriliş — Ay")[0];
    fireEvent.change(bulkIssueDay, { target: { value: "20" } });
    fireEvent.change(bulkIssueMonth, { target: { value: "9" } });
    expect(bulkIssueDay.closest(".compact-date-cell")).toHaveAttribute(
      "title",
      `20-Sep-${String(new Date().getFullYear()).slice(-2)}`,
    );

    expect(
      screen.queryByRole("columnheader", { name: "Inspection Description" }),
    ).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "Otomatik alanları göster" }),
    );
    expect(
      screen.getByRole("columnheader", { name: "Inspection Description" }),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Tümünü seç" }));
    fireEvent.change(screen.getByLabelText("Değiştirilecek alan"), {
      target: { value: "subcontractor" },
    });
    fireEvent.change(screen.getByLabelText("Yeni değer"), {
      target: { value: "PDL" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Seçilenlere uygula" }),
    );

    await waitFor(() => {
      const subcontractors = Array.from(
        document.querySelectorAll<HTMLInputElement>(
          'input[list^="editor-"][list$="-subcontractor"]',
        ),
        (input) => input.value,
      );
      expect(subcontractors).toEqual(["PDL", "PDL"]);
    });
  });

  it("selects a whole print group and includes or excludes it in one action", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("Yazdırma kuyruğunu kontrol et ve özelleştir"),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Tümünü seç" }));
    expect(screen.getByLabelText("Değiştirilecek alan")).toHaveValue("enabled");
    fireEvent.change(screen.getByLabelText("Yeni değer"), {
      target: { value: "false" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Seçilenlere uygula" }),
    );

    for (const checkbox of screen.getAllByLabelText("Yazdır")) {
      expect(checkbox).not.toBeChecked();
    }
  });

  it("opens preferences before authentication", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "DocFlow" });
    fireEvent.click(screen.getByRole("button", { name: "Ayarlar" }));
    expect(
      await screen.findByRole("heading", { name: "Ayarlar" }),
    ).toBeVisible();
    expect(screen.getByText("Dil")).toBeVisible();
    expect(screen.getByText("Tema")).toBeVisible();
  });
});
