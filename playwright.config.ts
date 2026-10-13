import {defineConfig} from '@playwright/test';
export default defineConfig({testDir:'./tests',workers:1,timeout:60000,reporter:[['list'],['json',{outputFile:'artifacts/ui-tests.json'}]],use:{trace:'retain-on-failure'}});
