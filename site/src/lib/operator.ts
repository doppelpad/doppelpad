// Who runs the website, for the legal notice (Impressum) and the privacy policy in both
// languages. Fill in every required `null` before the site goes public; until then the pages
// show a highlighted placeholder and a notice, and the build warns. An optional field left
// `null` is simply not shown.

export const OPERATOR: Record<keyof typeof FIELDS, string | null> = {
  name: 'Julian Zameit',
  street: 'Im Schneppenkamp 2',
  city: '45525 Hattingen',
  country: 'Deutschland',
  email: 'julian.zameit.96@gmail.com',
  phone: null,
};

/** What each field holds, for the placeholders, and whether the pages can do without it. */
export const FIELDS = {
  name: { de: 'Vor- und Nachname', en: 'full name' },
  street: { de: 'Straße und Hausnummer', en: 'street and number' },
  city: { de: 'Postleitzahl und Ort', en: 'postcode and city' },
  country: { de: 'Land', en: 'country' },
  email: { de: 'E-Mail-Adresse', en: 'email address' },
  phone: { de: 'Telefonnummer', en: 'phone number', optional: true },
} as const;

export type Field = keyof typeof FIELDS;

export const isOptional = (k: Field) => 'optional' in FIELDS[k];

export const missingFields = () => (Object.keys(OPERATOR) as Field[]).filter((k) => !OPERATOR[k] && !isOptional(k));
