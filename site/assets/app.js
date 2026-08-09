/* ROrganizer — préférence de thème et de langue.
   Chargé sans `defer` dans le <head> : le thème est posé sur <html> avant le
   premier rendu, donc il n'y a pas de clignotement clair→sombre. */
(function () {
  'use strict';

  var LANGS = ['fr', 'en', 'es'];
  var FALLBACK = 'en';
  var THEME_KEY = 'rorganizer.theme';
  var LANG_KEY = 'rorganizer.lang';

  /* localStorage jette en navigation privée sur certains navigateurs et
     quand les cookies tiers sont bloqués : tout accès est encadré. */
  function read(key) {
    try {
      return localStorage.getItem(key);
    } catch (e) {
      return null;
    }
  }

  function write(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (e) {
      /* préférence non mémorisable : le site reste utilisable */
    }
  }

  /* ------------------------------------------------------------- thème */

  var savedTheme = read(THEME_KEY);
  if (savedTheme === 'dark' || savedTheme === 'light') {
    document.documentElement.setAttribute('data-theme', savedTheme);
  }

  function currentTheme() {
    var explicit = document.documentElement.getAttribute('data-theme');
    if (explicit) {
      return explicit;
    }
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  /* ------------------------------------------------------------- langue */

  /* Parcours de `navigator.languages` dans l'ordre, comparaison sur la
     sous-étiquette primaire. Un `startsWith` sur la chaîne entière raterait
     `es-419` et `es-MX`, et l'ordre importe : des préférences
     ["de-DE", "fr"] doivent mener au français, pas au repli anglais. */
  function preferredLang() {
    var chosen = read(LANG_KEY);
    if (LANGS.indexOf(chosen) !== -1) {
      return chosen;
    }

    var list = navigator.languages && navigator.languages.length
      ? navigator.languages
      : [navigator.language || ''];

    for (var i = 0; i < list.length; i++) {
      var primary = String(list[i]).toLowerCase().split('-')[0];
      if (LANGS.indexOf(primary) !== -1) {
        return primary;
      }
    }
    return FALLBACK;
  }

  /* La racine se signale par un attribut sur <html>, disponible dès
     l'exécution de ce script — pas par une inspection du chemin, qui ne
     fonctionnerait pas en aperçu local `file://`.
     Cible relative pour la même raison : `fr/` résout correctement dans les
     deux contextes.
     `replace()` et non `href` : sinon la racine reste dans l'historique et le
     bouton Retour reboucle indéfiniment sur la redirection. */
  if (document.documentElement.hasAttribute('data-gate')) {
    location.replace(preferredLang() + '/');
    return;
  }

  /* ---------------------------------------------------------- interactions */

  document.addEventListener('DOMContentLoaded', function () {
    var toggle = document.querySelector('[data-theme-toggle]');
    if (toggle) {
      toggle.addEventListener('click', function () {
        var next = currentTheme() === 'dark' ? 'light' : 'dark';
        document.documentElement.setAttribute('data-theme', next);
        write(THEME_KEY, next);
      });
    }

    /* Un choix explicite de langue doit primer sur la détection au prochain
       passage par la racine. */
    var links = document.querySelectorAll('.langset a[hreflang]');
    for (var i = 0; i < links.length; i++) {
      links[i].addEventListener('click', function () {
        write(LANG_KEY, this.getAttribute('hreflang'));
      });
    }
  });
})();
