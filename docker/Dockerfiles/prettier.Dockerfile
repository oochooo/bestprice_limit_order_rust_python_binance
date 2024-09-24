FROM node:latest
USER root
RUN npm install --global prettier
WORKDIR /code