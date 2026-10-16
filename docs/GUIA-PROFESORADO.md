# Corregir — guía rápida para o profesorado

Corregir axuda a corrixir traballos escritos cunha IA que se executa no teu propio equipo. A nota ponla sempre ti: a IA só localiza fragmentos do texto e propón un borrador de retroalimentación que ti revisas e editas.

> Mentres o centro e o delegado/a de protección de datos (DPD) non validen o uso con datos reais, utiliza só datos sintéticos.

## Antes de empezar

- Debes ter instalado [Ollama](https://ollama.com) e o modelo indicado en **Configuración** (por defecto `qwen3:8b`). Comproba a conexión co botón **Probar a conexión coa IA**.
- Escolle o idioma na parte superior dereita. Os informes e as follas de notas saen no idioma da interface.
- En **Configuración** podes escoller tamén o idioma dos borradores de retroalimentación. Por defecto usan o mesmo idioma ca resposta do alumno/a.

## 1. Prepara a clase e a rúbrica

1. En **Clases**, crea o grupo e pega a lista do alumnado, un nome completo por liña. Podes copiar unha columna dunha folla de cálculo ou dunha exportación de XADE.
2. En **Rúbricas**, crea os criterios coa súa puntuación máxima. Se queres, engade **niveis de logro** a cada criterio (por exemplo, Excelente 4, Parcial 2, Aínda non 0): ao corrixir abondará con premer no nivel.
3. Se cambias unha rúbrica, gárdase unha versión nova. As tarefas que xa a usaban conservan a versión antiga.

## 2. Crea a tarefa e importa as entregas

1. En **Entregas**, preme **Nova tarefa**. Indica o título, a rúbrica, a clase e o enunciado.
2. Preme **Importar documentos** e selecciona todos os ficheiros á vez (PDF, DOCX só con texto, PNG ou JPEG). Procésanse un a un e vese o progreso.
3. Se un traballo manuscrito non se recoñeceu ben, activa **Usar OCR en todas as páxinas do PDF** e importa de novo ese ficheiro.

Os documentos orixinais non se copian: quedan onde estaban.

## 3. Revisa cada entrega

Selecciona unha entrega na lista da esquerda. Para cada unha:

1. **Comparar o orixinal co texto extraído** (opcional): abre o PDF ou a imaxe orixinal e corrixe os erros de recoñecemento sen cambiar a resposta do alumno/a. Garda os cambios.
2. **Anonimizar identidades** (obrigatorio):
   - A aplicación propón o alumno/a a partir do nome do ficheiro ou do texto. Comproba que é correcto.
   - Revisa os identificadores detectados (nomes, DNI, correos, teléfonos…) e engade calquera outro fragmento que poida identificar ao alumno/a.
   - Preme **Ver o texto anonimizado**, le o texto que recibirá a IA e marca **Revisei este texto e non quedan identificadores**.
   - Preme **Confirmar anonimización**.

A IA nunca recibe o nome do alumno/a: só o texto anonimizado cun alias.

## 4. Corrixe

1. **Buscar evidencias**: a IA sinala, para cada criterio, fragmentos literais da resposta. Non propón notas. Podes pedilo para todas as entregas listas co botón **Buscar evidencias en todas**.
2. Pon a túa puntuación en cada criterio (ou preme nun nivel) e, se queres, un comentario. Vese o total e a nota sobre 10. Antes de enviar os comentarios á IA, o nome do alumno/a substitúese polo alias e os nomes dos compañeiros da clase ocúltanse; evita alcumes e outros datos persoais.
3. **Gardar a miña avaliación**.
4. **Xerar borrador de retroalimentación**: a IA redacta un texto dirixido ao alumno/a a partir das túas puntuacións e comentarios, e sinala posibles incoherencias para que as revises. O borrador aparece nun campo editable: corríxeo ou substitúeo polo teu texto.
5. **Confirmar nota**. Se tes cambios sen gardar na retroalimentación, gárdanse antes.

Se te equivocas despois de confirmar, preme **Reabrir nota** e indica o motivo; queda rexistrado na auditoría.

## 5. Exporta

- **Exportar notas confirmadas (CSV)**: unha fila por alumno/a, unha columna por criterio, total e nota sobre 10. Ábrese directamente nunha folla de cálculo en galego ou castelán.
- **Exportar informe (PDF)** dunha entrega, ou **Exportar todos os informes (PDF)** a un cartafol. O informe indica se a retroalimentación se redactou con axuda da IA.

Estes ficheiros conteñen nomes e notas: gárdaos con coidado.

## 6. Privacidade e conservación

En **Privacidade**:

- Indica o **prazo de conservación** acordado co DPD. A aplicación avisará cando unha tarefa o supere.
- **Eliminar identidades** dunha tarefa rematada: bórranse os nomes, o texto sen anonimizar e os nomes de ficheiro; conservas as notas e a retroalimentación.
- **Solicitudes de acceso e supresión**: exporta todos os datos dun alumno/a (JSON) ou bórraos por completo. Ao borrar, o seu nome retírase tamén dos comentarios e da retroalimentación doutros alumnos.

As copias de seguranza anteriores poden conter aínda datos borrados. Despois dunha supresión, crea unha copia nova e elimina as antigas.

## 7. Copias de seguranza

En **Configuración → Copias de seguranza e recuperación** crea copias cifradas cun contrasinal propio (mínimo 12 caracteres). Garda o contrasinal á parte: sen el non se pode recuperar a copia. A aplicación lémbrache facer unha copia cando a última ten máis de sete días.
