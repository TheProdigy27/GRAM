// Évite l'ouverture d'une console en plus de la fenêtre en release. NE PAS SUPPRIMER.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gram_lib::run()
}
