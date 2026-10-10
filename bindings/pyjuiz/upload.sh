#!/bin/bash


rm -rf dist/* pyjuiz.egg-info/*

python3 setup.py sdist bdist_wheel

twine upload --repository pypi dist/*