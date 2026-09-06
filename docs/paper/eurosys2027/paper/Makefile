# Anonymous EuroSys 2027 fall PDF.
# Requires: pdflatex, bibtex, acmart (TeX Live publishers).
LATEX  ?= pdflatex
BIBTEX ?= bibtex
PAPER  := main

.PHONY: all pdf clean anonymity-check

all: pdf anonymity-check

pdf: $(PAPER).pdf

$(PAPER).pdf: $(PAPER).tex paper.bib
	$(LATEX) -interaction=nonstopmode -halt-on-error $(PAPER).tex
	$(BIBTEX) $(PAPER)
	$(LATEX) -interaction=nonstopmode -halt-on-error $(PAPER).tex
	$(LATEX) -interaction=nonstopmode -halt-on-error $(PAPER).tex
	@pdfinfo $(PAPER).pdf | grep -E 'Pages|Title' || true

# Fail if the PDF source still contains identity strings that must not ship.
anonymity-check:
	@! grep -nE 'RayNu|raynuv|vikkp|Pandey|ORCID|github.com|RAYNU-V' \
	    $(PAPER).tex paper.bib ../supplement/*.tex 2>/dev/null \
	    | grep -vE '^#' || true
	@if grep -nE 'RayNu|raynuv|vikkp|Pandey|ORCID|github.com/vikkp|RAYNU-V' \
	    $(PAPER).tex paper.bib ../supplement/*.tex 2>/dev/null; then \
	  echo 'ANON FAIL: identity string in submission TeX'; exit 1; \
	else echo 'ANON OK: no identity strings in TeX/bib'; fi

clean:
	rm -f $(PAPER).aux $(PAPER).bbl $(PAPER).blg $(PAPER).log \
	      $(PAPER).out $(PAPER).toc $(PAPER).fls $(PAPER).fdb_latexmk \
	      $(PAPER).synctex.gz
