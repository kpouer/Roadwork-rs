# Extension Roadwork WME

Extension Chrome / userscript pour **Waze Map Editor (WME)** facilitant le suivi et l'intégration des chantiers 
routiers issus de sources Open Data.

## Fonctionnalités clés

Le plugin se présente sous forme d'une toolbar
![media/toolbar.png](media/toolbar.png).
Cliquez sur le bouton qui vous intéresse pour afficher l'outil correspondant.

### Travaux

Le plugin peut afficher des travaux routiers sur la carte WME, issu de sources Open Data.
Il est possible de gérer un status, par exemple pour marquer les chantiers comme *Traité* ou *Ignoré*.
Certaines villes et départements sont déjà prises en charge, d'autres suivront ainsi que la possibilité de l'étendre
soi même.
La synchronisation des statuts devrait être possible dans une version ultérieure.

### Polygones

Sur la fenêtre Polygones vous pouvez dropper un fichier WKT (Well-Known Text) pour afficher des polygones personnalisés.

### Données

- Il est possible d'importer n'importe quel fichier json contenant des informations géographiques. Feux rouges, passages à niveau ...
Pour cela dans la fenêtre "données", cliquez sur le bouton "Créer".
- Le système d'import s'ouvre.
- ![media/data_wizard1.png](media/data_wizard1.png)
- Indiquez une url ou droppez un fichier json (ou mieux, un GeoJSON).
- Le système d'import vous propose ensuite de choisir les champs à importer.
  - Il est indispensable d'indiquer ou se trouve le tableau de données (data_array), latitude et longitude
  - Ensuite donnez un nom à la source puis sauvegardez.
  - ![media/data_wizard2.png](media/data_wizard2.png)

L'outil tentera de détecter les données adéquates, et vous pouvez les ajuster.

Exemple, feux  tricolores à Paris
![media/feutricolore.png](media/feutricolore.png)

## Architecture

- **Moteur Rust / WebAssembly**
- **Stockage local SQLite (OPFS)**