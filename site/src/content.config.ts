import { defineCollection } from 'astro:content';
import { i18nLoader } from '@astrojs/starlight/loaders';
import { docsSchema, i18nSchema } from '@astrojs/starlight/schema';
import { docsWithHandbook } from './loaders/handbook';

export const collections = {
  docs: defineCollection({ loader: docsWithHandbook(), schema: docsSchema() }),
  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};
