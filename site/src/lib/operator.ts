// Who runs the website, for the legal notice (Impressum) and the privacy policy in both
// languages. Fill in every `null` before the site goes public; until then the pages show a
// highlighted placeholder and a notice, and the build warns.

export const OPERATOR: Record<keyof typeof FIELDS, string | null> = {
  name: null,
  street: null,
  city: null,
  country: null,
  email: null,
  phone: null,
};

/** What each field holds, for the placeholders. */
export const FIELDS = {
  name: { de: 'Vor- und Nachname', en: 'full name' },
  street: { de: 'Straße und Hausnummer', en: 'street and number' },
  city: { de: 'Postleitzahl und Ort', en: 'postcode and city' },
  country: { de: 'Land', en: 'country' },
  email: { de: 'E-Mail-Adresse', en: 'email address' },
  phone: { de: 'Telefonnummer oder ein weiterer schneller Kontaktweg', en: 'phone number or another quick way to get in touch' },
} as const;

export const missingFields = () => (Object.keys(OPERATOR) as (keyof typeof FIELDS)[]).filter((k) => !OPERATOR[k]);
